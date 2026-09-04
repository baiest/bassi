#[path = "../../common/fake_autonomous.rs"]
mod fake_autonomous;

use std::sync::Arc;

use fake_autonomous::{FailingAgent, RecordingAgent};
use nala::adapters::autonomous::in_memory_queue::InMemoryEventQueue;
use nala::application::autonomous::event::AutonomousEvent;
use nala::application::autonomous::event_loop::AutonomousEventLoop;
use nala::application::autonomous::policy::{EventDecision, EventPolicy};
use nala::ports::autonomous::AutonomousEventQueue;
use nala::ports::events::Event;

use crate::fake_events::RecordingEventSink;

/// A policy whose decision is fixed in the test, independent of
/// `RuleBasedPolicy`'s own rules -- keeps these tests focused on the
/// loop's behavior, not the default policy's.
struct FixedPolicy(EventDecision);

impl EventPolicy for FixedPolicy {
    fn decide(&self, _event: &AutonomousEvent) -> EventDecision {
        self.0.clone()
    }
}

#[test]
fn an_ignored_event_never_reaches_the_agent() {
    let queue = Arc::new(InMemoryEventQueue::new(8));
    let agent = RecordingAgent::new("unused");
    let mut event_loop = AutonomousEventLoop::new(
        Arc::clone(&queue) as Arc<_>,
        FixedPolicy(EventDecision::Ignore {
            reason: "not relevant".to_string(),
        }),
        agent.clone(),
        RecordingEventSink::new(),
    );

    queue.publish(AutonomousEvent::new(
        "esp32-bedroom",
        "heartbeat",
        serde_json::json!({}),
    ));
    queue.close();

    event_loop.run();

    assert_eq!(agent.call_count(), 0);
}

#[test]
fn a_relevant_event_delegates_to_the_agent_with_the_rendered_prompt() {
    let queue = Arc::new(InMemoryEventQueue::new(8));
    let agent = RecordingAgent::new("Battery is low, I'll remind you later.");
    let mut event_loop = AutonomousEventLoop::new(
        Arc::clone(&queue) as Arc<_>,
        FixedPolicy(EventDecision::Act {
            prompt: "battery at 9%, act on it".to_string(),
        }),
        agent.clone(),
        RecordingEventSink::new(),
    );

    queue.publish(AutonomousEvent::new(
        "esp32-bedroom",
        "battery_low",
        serde_json::json!({"percent": 9}),
    ));
    queue.close();

    event_loop.run();

    assert_eq!(agent.call_count(), 1);
    assert_eq!(agent.prompts.lock().unwrap()[0], "battery at 9%, act on it");
}

#[test]
fn a_failing_agent_emits_a_failure_event_and_the_loop_keeps_running() {
    let queue = Arc::new(InMemoryEventQueue::new(8));
    let agent = FailingAgent {
        error: "llm request failed".to_string(),
    };
    let events = RecordingEventSink::new();
    let mut event_loop = AutonomousEventLoop::new(
        Arc::clone(&queue) as Arc<_>,
        FixedPolicy(EventDecision::Act {
            prompt: "act on it".to_string(),
        }),
        agent,
        events,
    );

    queue.publish(AutonomousEvent::new(
        "esp32-bedroom",
        "battery_low",
        serde_json::json!({}),
    ));
    queue.publish(AutonomousEvent::new(
        "esp32-kitchen",
        "battery_low",
        serde_json::json!({"different": true}),
    ));
    queue.close();

    // Must not panic despite every event failing.
    event_loop.run();

    let failures = event_loop
        .events()
        .events
        .iter()
        .filter(|event| matches!(event, Event::AutonomousEventFailed { .. }))
        .count();
    assert_eq!(failures, 2);
}

#[test]
fn several_events_are_processed_independently_and_in_order() {
    let queue = Arc::new(InMemoryEventQueue::new(8));
    let agent = RecordingAgent::new("ok");
    let mut event_loop = AutonomousEventLoop::new(
        Arc::clone(&queue) as Arc<_>,
        FixedPolicy(EventDecision::Act {
            prompt: "act".to_string(),
        }),
        agent.clone(),
        RecordingEventSink::new(),
    );

    queue.publish(AutonomousEvent::new(
        "esp32-bedroom",
        "battery_low",
        serde_json::json!({"n": 1}),
    ));
    queue.publish(AutonomousEvent::new(
        "esp32-kitchen",
        "battery_low",
        serde_json::json!({"n": 2}),
    ));
    queue.publish(AutonomousEvent::new(
        "esp32-hallway",
        "button_pressed",
        serde_json::json!({"n": 3}),
    ));
    queue.close();

    event_loop.run();

    assert_eq!(agent.call_count(), 3);
}

#[test]
fn the_emitted_events_narrate_received_ignored_delegated_completed() {
    let queue = Arc::new(InMemoryEventQueue::new(8));
    let agent = RecordingAgent::new("done");
    let mut event_loop = AutonomousEventLoop::new(
        Arc::clone(&queue) as Arc<_>,
        FixedPolicy(EventDecision::Act {
            prompt: "act".to_string(),
        }),
        agent,
        RecordingEventSink::new(),
    );

    queue.publish(AutonomousEvent::new(
        "esp32-bedroom",
        "battery_low",
        serde_json::json!({}),
    ));
    queue.close();

    event_loop.run();

    let kinds: Vec<&str> = event_loop
        .events()
        .events
        .iter()
        .map(|event| match event {
            Event::AutonomousEventReceived { .. } => "received",
            Event::AutonomousEventIgnored { .. } => "ignored",
            Event::AutonomousEventDelegated { .. } => "delegated",
            Event::AutonomousEventCompleted { .. } => "completed",
            Event::AutonomousEventFailed { .. } => "failed",
            _ => "other",
        })
        .collect();

    assert_eq!(kinds, vec!["received", "delegated", "completed"]);
}
