/// Whether the current turn should stop as soon as possible. Checked at
/// loop boundaries (before an LLM call, before/after each tool call,
/// between retries) rather than pre-emptively — a turn is aborted cleanly
/// between steps, not interrupted mid-step.
pub trait CancelSignal {
    fn is_cancelled(&self) -> bool;
}

/// Never cancelled. Used where a caller doesn't need real cancellation
/// (e.g. the planning call, or callers that don't wire up a signal).
pub struct NeverCancelled;

impl CancelSignal for NeverCancelled {
    fn is_cancelled(&self) -> bool {
        false
    }
}
