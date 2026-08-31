use chrono::{DateTime, Local};

/// Wall-clock, calendar time — distinct from `ports::clock::Clock`, which is
/// `Instant`-based and only used for timeouts/backoff. Tools that need to
/// know the actual current date/time (e.g. `current_time`) go through this
/// instead, so they can be driven deterministically in tests with a fixed
/// value.
pub trait WallClock {
    fn now_local(&self) -> DateTime<Local>;
}
