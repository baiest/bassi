//! The composition root for the voice front end: builds the speech backend,
//! wraps it around a narrator, and connects to Nala as a separate process.
//! `main.rs` only runs the result.

use agent_protocol::EventSink;

use tts::{AsyncSpeech, ChatterboxSupervisor, speech_backend};

use crate::client::{ClientError, NalaClient, TcpWire};
use crate::narration::TemplateNarrator;
use crate::speaking_sink::SpeakingEventSink;

/// A sink with nothing further to forward events to — Voice's only local
/// consumer of events is narration; per-task metrics now live server-side,
/// next to the agent that actually produces them.
pub struct NoopEventSink;

impl EventSink for NoopEventSink {
    fn emit(&mut self, _event: agent_protocol::Event) {}
}

pub type Events = SpeakingEventSink<NoopEventSink, TemplateNarrator>;

/// Default address `voice` connects to, overridable with `NALA_ADDR` —
/// matches `nala --serve`'s own default.
const DEFAULT_ADDR: &str = "127.0.0.1:4180";

/// Builds the speech backend, an `AsyncSpeech` handle to it, the narrating
/// event sink, and connects to Nala over WebSocket. Returns the
/// `AsyncSpeech` handle separately so `main` can speak the final answer
/// after a turn completes, and the `ChatterboxSupervisor` (if one was
/// started) so `main` can keep it alive.
pub fn build() -> Result<
    (
        NalaClient<TcpWire>,
        Events,
        AsyncSpeech,
        Option<ChatterboxSupervisor>,
    ),
    ClientError,
> {
    let (backend, chatterbox_supervisor) = speech_backend();
    let speech = AsyncSpeech::new(backend);

    let events = SpeakingEventSink::new(NoopEventSink, TemplateNarrator::new(), speech.clone());

    let addr = std::env::var("NALA_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());
    let client = NalaClient::new(TcpWire::connect(&addr)?);

    Ok((client, events, speech, chatterbox_supervisor))
}

/// Loads the Whisper model once at startup. Loading is slow (reads the
/// whole model file), so `main` builds one `Transcriber` and reuses it
/// across turns rather than reloading it per turn.
pub fn build_transcriber() -> stt::Transcriber {
    let model_path = std::env::var("NALA_WHISPER_MODEL")
        .unwrap_or_else(|_| "data/whisper/ggml-small.bin".to_string());

    stt::Transcriber::load(&model_path).expect("Failed to load Whisper model")
}
