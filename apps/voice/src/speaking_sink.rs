use nala::ports::events::{Event, EventSink};
use tts::AsyncSpeech;

use crate::narrator::Narrator;

/// Decorates any `EventSink` so turn events also get spoken aloud, via a
/// `Narrator` that decides what (if anything) to say and an `AsyncSpeech`
/// that speaks it without blocking the agent loop. Wraps rather than
/// replaces the inner sink — e.g. `ConsoleEventSink` still prints
/// everything exactly as before.
pub struct SpeakingEventSink<E, N> {
    inner: E,
    narrator: N,
    speech: AsyncSpeech,
}

impl<E, N> SpeakingEventSink<E, N> {
    pub fn new(inner: E, narrator: N, speech: AsyncSpeech) -> Self {
        Self {
            inner,
            narrator,
            speech,
        }
    }

    /// The wrapped sink, for callers (mainly tests) that need to inspect
    /// what was forwarded to it.
    pub fn inner(&self) -> &E {
        &self.inner
    }
}

impl<E, N> EventSink for SpeakingEventSink<E, N>
where
    E: EventSink,
    N: Narrator,
{
    fn emit(&mut self, event: Event) {
        if let Some(phrase) = self.narrator.narrate(&event) {
            self.speech.say_narration(&phrase);
        }
        self.inner.emit(event);
    }
}
