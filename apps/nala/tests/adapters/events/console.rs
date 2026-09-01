use std::time::Duration;

use nala::{
    adapters::events::console::ConsoleEventSink,
    ports::events::{Event, EventSink, LlmCallId, TaskId},
};

#[test]
fn should_emit_event() {
    let mut sink = ConsoleEventSink;

    sink.emit(Event::RequestStarted {
        task_id: TaskId::new(),
    });
}

#[test]
fn should_emit_greeting_event() {
    let mut sink = ConsoleEventSink;

    sink.emit(Event::Greeting {
        text: "Hola, en que te puedo ayudar?".to_string(),
    });
}

#[test]
fn should_emit_llm_failed_event() {
    let mut sink = ConsoleEventSink;

    let task_id = TaskId::new();
    let llm_call_id = LlmCallId::new(&task_id, 1);
    sink.emit(Event::LlmFailed {
        task_id,
        llm_call_id,
        call_index: 1,
        duration: Duration::from_millis(5),
        error: "connection refused".to_string(),
    });
}
