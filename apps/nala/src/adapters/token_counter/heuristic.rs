use crate::ports::llm::Message;
use crate::ports::token_counter::TokenCounter;

/// Rough characters-per-token ratio for English/Spanish text with typical
/// tokenizers (~4 chars/token). Good enough to decide "are we close to the
/// limit", not meant to match any specific tokenizer exactly.
const CHARS_PER_TOKEN: usize = 4;

/// Flat token cost assumed per attached image, since actual image tokenization
/// varies by model and resolution and nala has no access to the model's
/// vision encoder. Deliberately generous (an image is usually the dominant
/// cost in a turn that has any) so eviction kicks in before a real
/// overflow, not after.
const TOKENS_PER_IMAGE: usize = 800;

/// A dependency-free token estimator: no tokenizer, just character counts
/// and a flat per-image cost. Calibrate `CHARS_PER_TOKEN`/`TOKENS_PER_IMAGE`
/// against `Usage.prompt_tokens` (from real `LlmResponse`s) if the estimate
/// drifts too far from what Ollama actually reports.
pub struct HeuristicTokenCounter;

impl HeuristicTokenCounter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for HeuristicTokenCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenCounter for HeuristicTokenCounter {
    fn estimate_message(&self, message: &Message) -> usize {
        let text_tokens = message.content.len().div_ceil(CHARS_PER_TOKEN);
        let image_tokens = message.images.len() * TOKENS_PER_IMAGE;
        text_tokens + image_tokens
    }
}
