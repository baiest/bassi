use std::time::Duration;

/// Reference values match the Android client's own reconnect loop
/// (`NalaService.kt`'s `RECONNECT_INITIAL_DELAY_MS`/`RECONNECT_MAX_DELAY_MS`)
/// so every device in the system backs off the same way.
pub const RECONNECT_INITIAL_DELAY: Duration = Duration::from_secs(1);
pub const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(30);

/// Doubling backoff with a floor and a ceiling. `next_delay` both returns
/// the delay to wait *before* the upcoming attempt and advances the state
/// for the attempt after that, so a caller just calls it once per retry.
pub struct Backoff {
    current: Duration,
    min: Duration,
    max: Duration,
}

impl Backoff {
    pub fn new(min: Duration, max: Duration) -> Self {
        Self {
            current: min,
            min,
            max,
        }
    }

    pub fn next_delay(&mut self) -> Duration {
        let delay = self.current;
        self.current = (self.current * 2).min(self.max);
        delay
    }

    /// Called after a connection succeeds, so the *next* failure starts
    /// backing off from `min` again instead of continuing from wherever a
    /// previous run of failures left off.
    pub fn reset(&mut self) {
        self.current = self.min;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnect_backoff_doubles_up_to_the_maximum() {
        let mut backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(30));

        let delays: Vec<Duration> = (0..8).map(|_| backoff.next_delay()).collect();

        assert_eq!(
            delays,
            vec![
                Duration::from_secs(1),
                Duration::from_secs(2),
                Duration::from_secs(4),
                Duration::from_secs(8),
                Duration::from_secs(16),
                Duration::from_secs(30),
                Duration::from_secs(30),
                Duration::from_secs(30),
            ]
        );
    }

    #[test]
    fn a_successful_connection_resets_the_backoff() {
        let mut backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(30));

        backoff.next_delay();
        backoff.next_delay();
        backoff.next_delay();
        backoff.reset();

        assert_eq!(backoff.next_delay(), Duration::from_secs(1));
    }
}
