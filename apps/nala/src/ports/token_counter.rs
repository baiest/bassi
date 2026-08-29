use crate::ports::llm::Message;

/// Estimates how many tokens a set of messages will cost, so the loop can
/// decide *before* sending a request whether it needs to trim the prompt.
/// This is necessarily an estimate — the only exact count comes back after
/// the request completes (see `Usage` on `LlmResponse`) — but it needs to
/// be cheap enough to run before every call.
pub trait TokenCounter {
    fn estimate_message(&self, message: &Message) -> usize;

    fn estimate(&self, messages: &[Message]) -> usize {
        messages
            .iter()
            .map(|message| self.estimate_message(message))
            .sum()
    }
}
