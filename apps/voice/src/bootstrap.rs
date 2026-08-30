//! The composition root for the voice front end: builds the speech backend,
//! wraps it around Nala's event chain, and builds the assistant on top of
//! it. `main.rs` only runs the result.

use std::path::PathBuf;
use std::sync::Arc;

use nala::adapters::events::console::ConsoleEventSink;
use nala::adapters::llm::ollama::OllamaLlm;
use nala::adapters::metrics::csv_sink::CsvMetricsSink;
use nala::application::assistant::Assistant;
use nala::application::tools::dispatcher::ToolDispatcher;
use nala::bootstrap::{self, ComputerType, DEFAULT_MODEL, McpClientType};

use tts::{AsyncSpeech, ChatterboxSupervisor, speech_backend};

use crate::narration::TemplateNarrator;
use crate::speaking_sink::SpeakingEventSink;

type Events = CsvMetricsSink<SpeakingEventSink<ConsoleEventSink, TemplateNarrator>>;
type VoiceAssistant = Assistant<OllamaLlm, ToolDispatcher<ComputerType, McpClientType>, Events>;

/// Builds the speech backend, an `AsyncSpeech` handle to it, and the fully
/// wired `Assistant` narrating through the same speech queue. Returns the
/// `AsyncSpeech` handle separately so `main` can speak the final answer
/// after `process()` returns, and the `ChatterboxSupervisor` (if one was
/// started) so `main` can keep it alive.
pub fn build() -> (VoiceAssistant, AsyncSpeech, Option<ChatterboxSupervisor>) {
    let (backend, chatterbox_supervisor) = speech_backend();
    let speech = AsyncSpeech::new(backend);

    let events = ConsoleEventSink;
    let events = SpeakingEventSink::new(events, TemplateNarrator::new(), speech.clone());

    let metrics_dir = std::env::var("NALA_METRICS_DIR").ok().map(PathBuf::from);
    let model = std::env::var("NALA_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());
    let events = CsvMetricsSink::new(events, metrics_dir, "ollama", &model);

    let assistant = bootstrap::build_assistant(events);

    (assistant, speech, chatterbox_supervisor)
}

type VoiceListener = stt::Listener<
    stt::MicStream,
    stt::SileroVad,
    stt::WhisperWake<Arc<stt::Transcriber>>,
    Arc<stt::Transcriber>,
>;

/// Opens the microphone and builds the full always-listening pipeline.
///
/// Wake-word checking and final-command transcription use *different*
/// loaded models on purpose: the wake check runs every 800ms on a growing
/// buffer, so it needs to be fast, while the final transcription only runs
/// once per utterance and needs to be accurate. Sharing one model (as an
/// earlier version of this did) meant `base` was slow enough on CPU that a
/// wake check could take longer than the 800ms interval it's supposed to
/// run at — the mic kept recording into the ring the whole time, so by the
/// time a check returned, the next one was already working with stale
/// audio. It looked like the pipeline had stopped listening.
///
/// `NALA_WHISPER_WAKE_MODEL` defaults to `tiny` for that reason.
/// `NALA_WHISPER_MODEL` (the final-transcription model) still defaults to
/// `base`, not `tiny`: measured on a real recording, `tiny` mistranscribed
/// "Nala" as "mala" on its own — too unreliable for telling the assistant
/// from the user's cat, who is also named Nala. See BAS-25 for the
/// measurement. `set_initial_prompt` in `Transcriber::transcribe` biases
/// *both* models toward "Nala", which is what makes `tiny` acceptable for
/// the wake check despite that earlier finding.
pub fn build_listener() -> VoiceListener {
    let model_path = std::env::var("NALA_WHISPER_MODEL")
        .unwrap_or_else(|_| "data/whisper/ggml-base.bin".to_string());
    let wake_model_path = std::env::var("NALA_WHISPER_WAKE_MODEL")
        .unwrap_or_else(|_| "data/whisper/ggml-tiny.bin".to_string());
    // whisper.cpp's own startup log (which would otherwise show the model
    // size/type) is silenced in Transcriber::load, so this is the only
    // visible confirmation of which models actually got loaded — including
    // an env var left over from an earlier session, which is exactly the
    // kind of thing that turns "slow" into "looks broken".
    println!("🧠 Modelo Whisper (comando): {model_path}");
    println!("🧠 Modelo Whisper (wake word): {wake_model_path}");
    let transcriber =
        Arc::new(stt::Transcriber::load(&model_path).expect("Failed to load Whisper model"));
    let wake_transcriber = Arc::new(
        stt::Transcriber::load(&wake_model_path).expect("Failed to load Whisper wake model"),
    );

    let audio = stt::MicStream::open().expect("Failed to open microphone");
    println!("🎤 Micrófono: {}", audio.device_name());

    let vad = stt::SileroVad::new().expect("Failed to build the voice activity detector");
    let wake = stt::WhisperWake::new(wake_transcriber).with_check_callback(|text, elapsed| {
        if !text.trim().is_empty() {
            println!("👂 Escuché: \"{text}\" ({:.2}s)", elapsed.as_secs_f32());
        } else {
            println!(
                "  [stt] wake check: {:.2}s (sin texto)",
                elapsed.as_secs_f32()
            );
        }
    });

    stt::Listener::new(
        audio,
        vad,
        wake,
        transcriber,
        stt::ListenerConfig::default(),
    )
}
