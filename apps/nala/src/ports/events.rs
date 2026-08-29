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
}

pub trait EventSink {
    fn emit(&mut self, event: Event);
}
