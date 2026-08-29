use std::time::Duration;

/// Every bound the agent loop enforces on a single turn, gathered in one
/// place instead of scattered `const`s. `Default` matches the values the
/// loop used before this existed; override via `from_env` or by
/// constructing one directly (e.g. in tests, to reach a limit quickly).
#[derive(Debug, Clone)]
pub struct LoopLimits {
    /// Total tool calls allowed in one turn, across every LLM round-trip.
    pub max_tool_calls: usize,
    /// How many times in a row the exact same tool call (name + arguments)
    /// can be requested before the turn is aborted as a loop.
    pub max_identical_repeats: usize,
    /// How many tool calls in a row may fail (dispatcher error, not a
    /// tool's own reported failure) before the turn gives up instead of
    /// burning the rest of `max_tool_calls` on a broken tool.
    pub max_consecutive_tool_failures: usize,
    /// How many times a retryable LLM failure is retried before the turn
    /// gives up.
    pub max_llm_retries: u32,
    /// Base delay for the retry backoff (attempt N waits
    /// `retry_base_delay * 2^N`).
    pub retry_base_delay: Duration,
}

impl Default for LoopLimits {
    fn default() -> Self {
        Self {
            max_tool_calls: 30,
            max_identical_repeats: 3,
            max_consecutive_tool_failures: 5,
            max_llm_retries: 3,
            retry_base_delay: Duration::from_millis(250),
        }
    }
}

impl LoopLimits {
    /// Applies `NALA_MAX_TOOL_CALLS`, `NALA_MAX_IDENTICAL_REPEATS`,
    /// `NALA_MAX_CONSECUTIVE_TOOL_FAILURES`, and `NALA_MAX_LLM_RETRIES` on
    /// top of the defaults, same pattern as `NALA_OLLAMA_NUM_CTX` in the
    /// Ollama adapter. Any var that's unset or doesn't parse keeps its
    /// default.
    pub fn from_env() -> Self {
        let defaults = Self::default();

        Self {
            max_tool_calls: env_usize("NALA_MAX_TOOL_CALLS").unwrap_or(defaults.max_tool_calls),
            max_identical_repeats: env_usize("NALA_MAX_IDENTICAL_REPEATS")
                .unwrap_or(defaults.max_identical_repeats),
            max_consecutive_tool_failures: env_usize("NALA_MAX_CONSECUTIVE_TOOL_FAILURES")
                .unwrap_or(defaults.max_consecutive_tool_failures),
            max_llm_retries: std::env::var("NALA_MAX_LLM_RETRIES")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(defaults.max_llm_retries),
            ..defaults
        }
    }
}

fn env_usize(key: &str) -> Option<usize> {
    std::env::var(key).ok().and_then(|value| value.parse().ok())
}
