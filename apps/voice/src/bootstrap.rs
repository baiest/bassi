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

    let metrics_dir = std::env::var("NALA_METRICS_DIR").ok().map(PathBuf::from);
    let model = std::env::var("NALA_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());
    let events = CsvMetricsSink::new(events, metrics_dir, "ollama", &model);

    let assistant = bootstrap::build_assistant(events);

    (assistant, speech, chatterbox_supervisor)
}
