use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};

use crate::application::autonomous::event::AutonomousEvent;
use crate::ports::autonomous::{AutonomousEventQueue, PublishOutcome};

struct State {
    events: VecDeque<AutonomousEvent>,
    closed: bool,
}

/// A bounded, in-process `AutonomousEventQueue` -- a `Mutex`-guarded
/// `VecDeque` with a `Condvar` so `next()` can block without spinning.
/// Sized for one process's worth of devices, not for durability: nothing
/// here survives a restart, matching the rest of this crate (the
/// transcript doesn't either).
pub struct InMemoryEventQueue {
    state: Mutex<State>,
    not_empty: Condvar,
    capacity: usize,
}

impl InMemoryEventQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            state: Mutex::new(State {
                events: VecDeque::new(),
                closed: false,
            }),
            not_empty: Condvar::new(),
            capacity,
        }
    }
}

impl AutonomousEventQueue for InMemoryEventQueue {
    fn publish(&self, event: AutonomousEvent) -> PublishOutcome {
        let mut state = self.state.lock().unwrap();

        if state.closed {
            return PublishOutcome::Dropped;
        }
        if state
            .events
            .iter()
            .any(|queued| queued.is_duplicate_of(&event))
        {
            return PublishOutcome::Duplicate;
        }
        if state.events.len() >= self.capacity {
            return PublishOutcome::Dropped;
        }

        state.events.push_back(event);
        self.not_empty.notify_one();
        PublishOutcome::Accepted
    }

    fn next(&self) -> Option<AutonomousEvent> {
        let mut state = self.state.lock().unwrap();
        loop {
            if let Some(event) = state.events.pop_front() {
                return Some(event);
            }
            if state.closed {
                return None;
            }
            state = self.not_empty.wait(state).unwrap();
        }
    }

    fn close(&self) {
        let mut state = self.state.lock().unwrap();
        state.closed = true;
        self.not_empty.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_event(kind: &str) -> AutonomousEvent {
        AutonomousEvent::new("esp32-bedroom", kind, serde_json::json!({}))
    }

    #[test]
    fn a_queue_with_room_accepts_a_new_event() {
        let queue = InMemoryEventQueue::new(4);

        assert_eq!(
            queue.publish(sample_event("button_pressed")),
            PublishOutcome::Accepted
        );
    }

    #[test]
    fn next_on_an_empty_closed_queue_returns_none_immediately() {
        let queue = InMemoryEventQueue::new(4);
        queue.close();

        assert!(queue.next().is_none());
    }
}
