use nala::{
    adapters::events::console::ConsoleEventSink,
    ports::events::{Event, EventSink},
};

#[test]
fn should_emit_event() {
    let mut sink = ConsoleEventSink;

    sink.emit(Event::RequestStarted);
}
