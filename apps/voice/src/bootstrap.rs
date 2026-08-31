//! The composition root for the voice front end: builds the speech backend,
//! wraps it around Nala's event chain, and builds the assistant on top of
//! it. `main.rs` only runs the result.

use std::path::PathBuf;

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

    // Defaults to data/metrics so every run gets token accounting without
    // extra setup; override with NALA_METRICS_DIR to point elsewhere.
    let metrics_dir = Some(
        std::env::var("NALA_METRICS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("data/metrics")),
    );
    let model = std::env::var("NALA_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());
    let events = CsvMetricsSink::new(events, metrics_dir, "ollama", &model);

    let assistant = bootstrap::build_assistant(events);

    (assistant, speech, chatterbox_supervisor)
}

/// Loads the Whisper model once at startup. Loading is slow (reads the
/// whole model file), so `main` builds one `Transcriber` and reuses it
/// across turns rather than reloading it per turn.
pub fn build_transcriber() -> stt::Transcriber {
    let model_path = std::env::var("NALA_WHISPER_MODEL")
        .unwrap_or_else(|_| "data/whisper/ggml-small.bin".to_string());

    stt::Transcriber::load(&model_path).expect("Failed to load Whisper model")
}
