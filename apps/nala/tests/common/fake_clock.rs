use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use nala::ports::clock::Clock;

/// Records every `sleep` request instead of actually waiting, so retry
/// backoff can be asserted on without slowing tests down.
#[derive(Default)]
pub struct FakeClock {
    pub sleeps: Rc<RefCell<Vec<Duration>>>,
}

impl FakeClock {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sleeps(&self) -> Rc<RefCell<Vec<Duration>>> {
        self.sleeps.clone()
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn sleep(&self, duration: Duration) {
        self.sleeps.borrow_mut().push(duration);
    }
}
