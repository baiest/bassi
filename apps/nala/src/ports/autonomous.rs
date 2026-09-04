use crate::application::autonomous::event::AutonomousEvent;

/// What happened when an `AutonomousEvent` was offered to the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishOutcome {
    Accepted,
    /// An equivalent event (see `AutonomousEvent::is_duplicate_of`) is
    /// already waiting to be processed.
    Duplicate,
    /// The queue is at capacity.
    Dropped,
}

/// Where autonomous events wait between being published (by a device
/// listener, a timer, ...) and being picked up by the `AutonomousEventLoop`.
/// `publish` never blocks -- a device server's reading thread must never
/// stall waiting on Nala's reasoning to catch up -- so a full queue drops
/// the newest event rather than backing up the publisher.
pub trait AutonomousEventQueue: Send + Sync {
    fn publish(&self, event: AutonomousEvent) -> PublishOutcome;

    /// Blocks until an event is available, or returns `None` once the
    /// queue has been closed and drained.
    fn next(&self) -> Option<AutonomousEvent>;

    /// Wakes any blocked `next()` call and makes every future one return
    /// `None`, for graceful shutdown.
    fn close(&self);
}

/// The bridge the `AutonomousEventLoop` uses to reuse the existing agent
/// loop without depending on `Assistant`'s generic parameters. Implemented
/// for `Assistant<L, D, E>` in `application::assistant`.
pub trait AutonomousAgent {
    fn respond_to(&mut self, prompt: &str) -> Result<String, String>;
}
