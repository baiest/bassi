use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use nala::ports::cancellation::CancelSignal;

/// A cancellation flag a test can flip mid-turn (e.g. from another thread,
/// to simulate Ctrl+C arriving while an LLM call is in flight). `Send` +
/// `Sync` so it can be flipped from outside the thread running `process`.
#[derive(Clone, Default)]
pub struct FakeCancelSignal {
    cancelled: Arc<AtomicBool>,
}

impl FakeCancelSignal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }
}

impl CancelSignal for FakeCancelSignal {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}
