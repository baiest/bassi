use crate::ports::llm::Message;
use crate::ports::token_counter::TokenCounter;

/// Configures how the per-turn prompt is kept within the model's context
/// window. `max_tokens` should match the `num_ctx` actually requested from
/// the backend (see `NALA_OLLAMA_NUM_CTX`) — this struct doesn't read that
/// env var itself to avoid coupling to one backend, but `from_env` applies
/// the same variable so the two stay in sync by default.
#[derive(Debug, Clone)]
pub struct ContextBudget {
    /// The backend's context window, in tokens.
    pub max_tokens: usize,
    /// Tokens set aside for the model's own reply, subtracted from
    /// `max_tokens` to get the budget available for the prompt.
    pub output_reserve: usize,
    /// How many of the most recent images are kept; older ones are
    /// stripped first since an image is almost always the dominant cost
    /// in a turn that has any (it vastly outweighs any text message) and
    /// a stale one is rarely still useful once newer ones exist.
    pub keep_recent_images: usize,
    /// A tool result's text longer than `truncate_head_chars +
    /// truncate_tail_chars` is cut down to a head/tail excerpt.
    pub truncate_head_chars: usize,
    pub truncate_tail_chars: usize,
    /// How many of the most recent messages are left untouched by
    /// compaction (see `Assistant::compact` in `agent_loop.rs`).
    pub keep_recent_uncompacted: usize,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            max_tokens: 8192,
            output_reserve: 1024,
            keep_recent_images: 2,
            truncate_head_chars: 400,
            truncate_tail_chars: 400,
            keep_recent_uncompacted: 6,
        }
    }
}

impl ContextBudget {
    /// Applies `NALA_OLLAMA_NUM_CTX` on top of the default, same variable
    /// `OllamaLlm` reads for `num_ctx` — so the budget matches what was
    /// actually requested from the backend unless told otherwise.
    pub fn from_env() -> Self {
        let defaults = Self::default();
        let max_tokens = std::env::var("NALA_OLLAMA_NUM_CTX")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(defaults.max_tokens);

        Self {
            max_tokens,
            ..defaults
        }
    }

    pub fn available_tokens(&self) -> usize {
        self.max_tokens.saturating_sub(self.output_reserve)
    }
}

/// Strips images from evictable tool-result messages older than the
/// `keep_recent_images` most recent ones. Returns how many images were
/// dropped (0 if nothing needed to change).
pub fn evict_images(
    messages: &mut [Message],
    protected_prefix: usize,
    keep_recent_images: usize,
) -> usize {
    let image_indices: Vec<usize> = messages
        .iter()
        .enumerate()
        .skip(protected_prefix)
        .filter(|(_, message)| !message.images.is_empty())
        .map(|(index, _)| index)
        .collect();

    if image_indices.len() <= keep_recent_images {
        return 0;
    }

    let drop_count = image_indices.len() - keep_recent_images;
    let mut dropped = 0;

    for &index in &image_indices[..drop_count] {
        dropped += messages[index].images.len();
        messages[index].images.clear();
    }

    dropped
}

/// Truncates any evictable `tool`-role message whose content is longer
/// than `head_chars + tail_chars` to a head/tail excerpt. Returns how many
/// messages were truncated.
pub fn truncate_long_tool_results(
    messages: &mut [Message],
    protected_prefix: usize,
    head_chars: usize,
    tail_chars: usize,
) -> usize {
    let mut truncated = 0;

    for message in messages.iter_mut().skip(protected_prefix) {
        if message.role != "tool" {
            continue;
        }

        let char_count = message.content.chars().count();
        if char_count <= head_chars + tail_chars {
            continue;
        }

        let head: String = message.content.chars().take(head_chars).collect();
        let tail: String = message
            .content
            .chars()
            .rev()
            .take(tail_chars)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();

        message.content = format!("{head}\n... [truncated] ...\n{tail}");
        truncated += 1;
    }

    truncated
}

/// Drops the oldest evictable messages, one at a time, until `messages`
/// fits `available_tokens` or nothing evictable is left. Returns how many
/// were dropped.
pub fn drop_oldest_until_fits(
    messages: &mut Vec<Message>,
    protected_prefix: usize,
    available_tokens: usize,
    counter: &dyn TokenCounter,
) -> usize {
    let mut dropped = 0;

    while messages.len() > protected_prefix && counter.estimate(messages) > available_tokens {
        messages.remove(protected_prefix);
        dropped += 1;
    }

    dropped
}
