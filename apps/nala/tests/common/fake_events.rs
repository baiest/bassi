use nala::ports::events::{Event, EventSink};

/// Records every emitted event so tests can inspect what happened during a
/// turn, e.g. how many images were attached to a tool result or sent to the
/// LLM.
#[derive(Default)]
pub struct RecordingEventSink {
    pub events: Vec<Event>,
}

impl RecordingEventSink {
    pub fn new() -> Self {
        Self::default()
    }
}

impl EventSink for RecordingEventSink {
    fn emit(&mut self, event: Event) {
        self.events.push(event);
    }
}
