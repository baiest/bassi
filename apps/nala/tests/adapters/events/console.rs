use std::time::Duration;

use nala::{
    adapters::events::console::ConsoleEventSink,
    ports::events::{Event, EventSink},
};

#[test]
fn should_emit_event() {
    let mut sink = ConsoleEventSink;

    sink.emit(Event::RequestStarted);
}

#[test]
fn should_emit_llm_failed_event() {
    let mut sink = ConsoleEventSink;

    sink.emit(Event::LlmFailed {
        duration: Duration::from_millis(5),
        error: "connection refused".to_string(),
    });
}
