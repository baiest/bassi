use std::sync::Arc;
use std::thread;
use std::time::Duration;

use nala::adapters::autonomous::in_memory_queue::InMemoryEventQueue;
use nala::application::autonomous::event::AutonomousEvent;
use nala::ports::autonomous::{AutonomousEventQueue, PublishOutcome};

fn sample_event(kind: &str) -> AutonomousEvent {
    AutonomousEvent::new("esp32-bedroom", kind, serde_json::json!({}))
}

#[test]
fn an_event_published_to_the_queue_comes_back_out_in_order() {
    let queue = InMemoryEventQueue::new(8);

    queue.publish(sample_event("button_pressed"));
    queue.publish(sample_event("battery_low"));

    let first = queue.next().expect("first event should be there");
    let second = queue.next().expect("second event should be there");

    assert_eq!(first.kind, "button_pressed");
    assert_eq!(second.kind, "battery_low");
}

#[test]
fn a_duplicate_event_already_queued_is_dropped() {
    let queue = InMemoryEventQueue::new(8);
    let event = sample_event("battery_low");

    assert_eq!(queue.publish(event.clone()), PublishOutcome::Accepted);
    assert_eq!(queue.publish(event), PublishOutcome::Duplicate);

    let first = queue.next().expect("the accepted event should be there");
    assert_eq!(first.kind, "battery_low");
}

#[test]
fn a_full_queue_drops_instead_of_blocking() {
    let queue = InMemoryEventQueue::new(1);

    assert_eq!(
        queue.publish(sample_event("button_pressed")),
        PublishOutcome::Accepted
    );
    assert_eq!(
        queue.publish(sample_event("battery_low")),
        PublishOutcome::Dropped
    );
}

#[test]
fn closing_the_queue_ends_the_loop() {
    let queue = Arc::new(InMemoryEventQueue::new(8));
    let waiting = Arc::clone(&queue);

    let handle = thread::spawn(move || waiting.next());

    // Give the consumer thread a moment to actually block on `next()`
    // before closing, so this test exercises the wake-on-close path
    // instead of racing it.
    thread::sleep(Duration::from_millis(50));
    queue.close();

    let result = handle.join().expect("consumer thread should not panic");
    assert!(result.is_none());
}
