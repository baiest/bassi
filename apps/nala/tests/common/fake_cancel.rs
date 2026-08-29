use std::cell::Cell;
use std::rc::Rc;

use nala::ports::cancellation::CancelSignal;

/// A cancellation flag a test can flip mid-turn (e.g. from inside a fake
/// LLM or tool, to simulate Ctrl+C arriving partway through).
#[derive(Clone, Default)]
pub struct FakeCancelSignal {
    cancelled: Rc<Cell<bool>>,
}

impl FakeCancelSignal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.set(true);
    }
}

impl CancelSignal for FakeCancelSignal {
    fn is_cancelled(&self) -> bool {
        self.cancelled.get()
    }
}
