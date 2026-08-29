use crate::ports::llm::Message;
use crate::ports::token_counter::TokenCounter;

/// The persisted conversation history across turns. Only complete text
/// turns are kept here — the tool calls and tool results a turn makes along
/// the way live only in that turn's local prompt (built fresh from the
/// transcript each time) and are not persisted, so history doesn't grow
/// unboundedly with tool chatter.
pub struct Transcript {
    messages: Vec<Message>,
}

impl Transcript {
    /// Seeds the transcript with the system prompt at index 0.
    pub fn new(system_prompt: Message) -> Self {
        Self {
            messages: vec![system_prompt],
        }
    }

    /// Appends a message, then drops the oldest non-system messages until
    /// the transcript's estimated token cost is back within `max_tokens`.
    /// The system prompt at index 0 is never pruned. Unlike the per-turn
    /// budget in `context_budget.rs` (images, truncation, compaction),
    /// pruning here is a plain drop — persisted history is text-only, so
    /// there are no images to strip and nothing worth truncating in place.
    pub fn push(&mut self, message: Message, counter: &dyn TokenCounter, max_tokens: usize) {
        self.messages.push(message);

        while self.messages.len() > 1 && counter.estimate(&self.messages) > max_tokens {
            self.messages.remove(1);
        }
    }

    /// A fresh clone of the persisted messages, for building this turn's
    /// prompt without mutating the transcript itself.
    pub fn snapshot(&self) -> Vec<Message> {
        self.messages.clone()
    }

    /// Number of persisted messages, including the system prompt.
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// The system prompt's content, if the (always-first) message is still
    /// the system prompt.
    pub fn system_prompt(&self) -> Option<&str> {
        self.messages
            .first()
            .filter(|message| message.role == "system")
            .map(|message| message.content.as_str())
    }
}
