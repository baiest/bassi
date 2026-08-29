use std::time::{Duration, Instant};

/// Time as seen by the agent loop — timeouts, deadlines, and retry backoff
/// all go through this instead of calling `std::time`/`std::thread::sleep`
/// directly, so they can be driven deterministically in tests with a
/// `FakeClock`.
pub trait Clock {
    fn now(&self) -> Instant;

    /// Blocks the calling thread for `duration`. A test double can record
    /// the requested duration instead of actually waiting.
    fn sleep(&self, duration: Duration);
}
