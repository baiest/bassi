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

/// Opens the microphone and builds the full always-listening pipeline:
/// VAD, wake-word detection and final-command transcription share one
/// loaded model (an `Arc<Transcriber>` — `Transcribe` only needs `&self`,
/// so this needs no lock) rather than loading it twice.
///
/// `NALA_WHISPER_MODEL` defaults to `base`, not `tiny`: measured on a real
/// recording, `tiny` mistranscribed "Nala" as "mala" — too unreliable for
/// telling the assistant from the user's cat, who is also named Nala. See
/// BAS-25 for the measurement.
pub fn build_listener() -> VoiceListener {
    let model_path = std::env::var("NALA_WHISPER_MODEL")
        .unwrap_or_else(|_| "data/whisper/ggml-base.bin".to_string());
    let transcriber =
        Arc::new(stt::Transcriber::load(&model_path).expect("Failed to load Whisper model"));

    let audio = stt::MicStream::open().expect("Failed to open microphone");
    println!("🎤 Micrófono: {}", audio.device_name());

    let vad = stt::SileroVad::new().expect("Failed to build the voice activity detector");
    let wake = stt::WhisperWake::new(Arc::clone(&transcriber)).with_check_callback(|text| {
        if !text.trim().is_empty() {
            println!("👂 Escuché: \"{text}\"");
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
