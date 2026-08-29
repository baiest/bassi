use std::time::Duration;

/// The phase the agent loop is currently in, for surfacing progress to the
/// UI (e.g. "Nala is thinking..." vs "Nala is executing..."). Transitions
/// are emitted as `Event::StateChanged`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnState {
    /// The user's input has been received and the turn is starting.
    Receiving,
    /// Generating the up-front step-by-step plan.
    Planning,
    /// Waiting on the LLM to decide the next action.
    Thinking,
    /// Running one or more tool calls the LLM requested.
    Executing,
    /// Mutating tool calls happened this turn and their effect hasn't been
    /// independently confirmed yet.
    Verifying,
    /// Producing the final natural-language answer.
    Responding,
}

#[derive(Debug)]
pub enum Event {
    RequestStarted,
    StateChanged {
        state: TurnState,
    },
    RequestCompleted {
        duration: Duration,
    },
    RequestFailed {
        duration: Duration,
        error: String,
    },

    /// The step-by-step plan the assistant generated for this request,
    /// before executing anything, from the user's request plus the
    /// available tools and computer context.
    PlanCreated {
        plan: String,
    },

    LlmStarted {
        /// Total images attached across the messages sent in this call
        /// (e.g. a screenshot from an earlier tool result), so it's visible
        /// from the outside that an image actually reached the model.
        images: usize,
    },
    LlmCompleted {
        duration: Duration,
    },

    ToolStarted {
        name: String,
        arguments: String,
    },
    ToolCompleted {
        name: String,
        duration: Duration,
        output: String,
        /// How many images the tool result carried (e.g. a screenshot).
        images: usize,
    },

    /// A retryable LLM failure is about to be retried after a backoff
    /// delay.
    Retrying {
        attempt: u32,
        error: String,
    },

    /// The turn was stopped because cancellation was requested (e.g.
    /// Ctrl+C) rather than because it finished or hit a limit.
    Cancelled,

    /// Real token accounting for a completed LLM call, when the backend
    /// reports it. Distinct from `LlmCompleted`'s `duration` — this is
    /// about context budget, not latency.
    TokensUsed {
        prompt_tokens: Option<u32>,
        completion_tokens: Option<u32>,
    },

    /// The prompt was over budget before a call and had to be trimmed.
    /// `remaining_estimate` is the estimated token count *after* this step,
    /// so repeated pressure events show whether trimming is converging.
    BudgetPressure {
        step: BudgetStep,
        remaining_estimate: usize,
    },

    /// Older turns were summarized into a single message to free up budget,
    /// after evicting images/text/whole turns wasn't enough on its own.
    TranscriptCompacted {
        turns_compacted: usize,
    },
}

/// Which eviction step fired in a `BudgetPressure` event, in the order the
/// budget fitter tries them — cheapest/least-lossy first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetStep {
    /// Images dropped from tool results older than the most recent few.
    DroppedImages { count: usize },
    /// Long tool-result text truncated to a head/tail excerpt.
    TruncatedText { count: usize },
    /// Whole oldest turns (a tool call and its result) dropped.
    DroppedTurns { count: usize },
}

pub trait EventSink {
    fn emit(&mut self, event: Event);
}
