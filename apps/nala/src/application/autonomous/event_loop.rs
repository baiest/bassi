//! The autonomous event loop's runner: pulls events off the queue, asks
//! the policy whether one is worth acting on, and if so delegates to the
//! existing agent loop through `AutonomousAgent`. Deliberately thin --
//! all the actual reasoning stays in `Assistant::process_from` (via
//! `AutonomousAgent::respond_to`); this only orchestrates.

use std::sync::Arc;
use std::time::Instant;

use crate::application::autonomous::event::AutonomousEvent;
use crate::application::autonomous::policy::{EventDecision, EventPolicy};
use crate::ports::autonomous::{AutonomousAgent, AutonomousEventQueue};
use crate::ports::events::{Event, EventSink};

pub struct AutonomousEventLoop<P, A, E> {
    queue: Arc<dyn AutonomousEventQueue>,
    policy: P,
    agent: A,
    events: E,
}

impl<P, A, E> AutonomousEventLoop<P, A, E>
where
    P: EventPolicy,
    A: AutonomousAgent,
    E: EventSink,
{
    pub fn new(queue: Arc<dyn AutonomousEventQueue>, policy: P, agent: A, events: E) -> Self {
        Self {
            queue,
            policy,
            agent,
            events,
        }
    }

    /// Blocks, processing events one at a time in arrival order, until the
    /// queue is closed and drained. Single-threaded consumption is what
    /// keeps events from interleaving/corrupting each other's state -- a
    /// user turn arriving concurrently runs on its own `Assistant` and
    /// thread entirely, untouched by this loop.
    pub fn run(&mut self) {
        while let Some(event) = self.queue.next() {
            self.process(event);
        }
    }

    fn process(&mut self, event: AutonomousEvent) {
        let event_id = event.id.to_string();

        self.events.emit(Event::AutonomousEventReceived {
            event_id: event_id.clone(),
            source: event.source.clone(),
            kind: event.kind.clone(),
        });

        match self.policy.decide(&event) {
            EventDecision::Ignore { reason } => {
                self.events
                    .emit(Event::AutonomousEventIgnored { event_id, reason });
            }
            EventDecision::Act { prompt } => {
                self.events.emit(Event::AutonomousEventDelegated {
                    event_id: event_id.clone(),
                });

                let started = Instant::now();
                match self.agent.respond_to(&prompt) {
                    Ok(reply) => {
                        self.events.emit(Event::AutonomousEventCompleted {
                            event_id,
                            duration: started.elapsed(),
                            reply,
                        });
                    }
                    Err(error) => {
                        self.events.emit(Event::AutonomousEventFailed {
                            event_id,
                            duration: started.elapsed(),
                            error,
                        });
                    }
                }
            }
        }
    }

    pub fn events(&self) -> &E {
        &self.events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::autonomous::in_memory_queue::InMemoryEventQueue;

    struct AlwaysIgnore;
    impl EventPolicy for AlwaysIgnore {
        fn decide(&self, _event: &AutonomousEvent) -> EventDecision {
            EventDecision::Ignore {
                reason: "test".to_string(),
            }
        }
    }

    struct NeverCalled;
    impl AutonomousAgent for NeverCalled {
        fn respond_to(&mut self, _prompt: &str) -> Result<String, String> {
            panic!("should never be called for an ignored event")
        }
    }

    #[derive(Default)]
    struct NullSink;
    impl EventSink for NullSink {
        fn emit(&mut self, _event: Event) {}
    }

    #[test]
    fn an_empty_closed_queue_returns_from_run_without_processing_anything() {
        let queue = Arc::new(InMemoryEventQueue::new(4));
        queue.close();

        let mut event_loop =
            AutonomousEventLoop::new(queue as Arc<_>, AlwaysIgnore, NeverCalled, NullSink);

        event_loop.run();
    }
}
