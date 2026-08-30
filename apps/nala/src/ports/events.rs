use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Identifies one `process()` call, so every event it emits — LLM calls,
/// tool calls, budget pressure, completion — can be correlated back to the
/// task that produced it (e.g. when writing them out as rows in a CSV keyed
/// by task).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskId(String);

impl TaskId {
    /// A process-wide unique id: current time in milliseconds plus a
    /// monotonic counter, so two tasks started in the same millisecond still
    /// get distinct ids. No UUID dependency needed — Nala is single-process,
    /// single-user, so global (cross-process) uniqueness isn't required.
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
        Self(format!("{millis}-{sequence}"))
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identifies a single LLM call within a task: `{task_id}-{call_index}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LlmCallId(String);

impl LlmCallId {
    pub fn new(task_id: &TaskId, call_index: u32) -> Self {
        Self(format!("{task_id}-{call_index}"))
    }
}

impl std::fmt::Display for LlmCallId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

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
    RequestStarted {
        task_id: TaskId,
    },
    StateChanged {
        task_id: TaskId,
        state: TurnState,
    },
    RequestCompleted {
        task_id: TaskId,
        duration: Duration,
    },
    RequestFailed {
        task_id: TaskId,
        duration: Duration,
        error: String,
    },

    /// The step-by-step plan the assistant generated for this request,
    /// before executing anything, from the user's request plus the
    /// available tools and computer context.
    PlanCreated {
        task_id: TaskId,
        plan: String,
    },

    LlmStarted {
        task_id: TaskId,
        llm_call_id: LlmCallId,
        /// This call's position within the task (1, 2, 3, ...), across
        /// every LLM call the task makes — the main loop, planning, and
        /// compaction summaries all share the same sequence.
        call_index: u32,
        /// Total images attached across the messages sent in this call
        /// (e.g. a screenshot from an earlier tool result), so it's visible
        /// from the outside that an image actually reached the model.
        images: usize,
    },
    LlmCompleted {
        task_id: TaskId,
        llm_call_id: LlmCallId,
        call_index: u32,
        duration: Duration,
    },
    /// A single LLM call failed — as opposed to `RequestFailed`, which marks
    /// the whole task giving up. One task can retry several failed calls
    /// (or absorb one in `build_plan`/`compact`, both non-fatal) without the
    /// task itself failing, so the two need separate events.
    LlmFailed {
        task_id: TaskId,
        llm_call_id: LlmCallId,
        call_index: u32,
        duration: Duration,
        error: String,
    },

    ToolStarted {
        task_id: TaskId,
        /// This tool call's position within the task (1, 2, 3, ...), across
        /// every tool call the task makes.
        tool_call_index: u32,
        name: String,
        arguments: String,
    },
    ToolCompleted {
        task_id: TaskId,
        tool_call_index: u32,
        name: String,
        duration: Duration,
        output: String,
        /// How many images the tool result carried (e.g. a screenshot).
        images: usize,
    },

    /// A retryable LLM failure is about to be retried after a backoff
    /// delay.
    Retrying {
        task_id: TaskId,
        attempt: u32,
        error: String,
    },

    /// The turn was stopped because cancellation was requested (e.g.
    /// Ctrl+C) rather than because it finished or hit a limit.
    Cancelled {
        task_id: TaskId,
    },

    /// Real token accounting for a completed LLM call, when the backend
    /// reports it. Distinct from `LlmCompleted`'s `duration` — this is
    /// about context budget, not latency.
    TokensUsed {
        task_id: TaskId,
        llm_call_id: LlmCallId,
        call_index: u32,
        prompt_tokens: Option<u32>,
        completion_tokens: Option<u32>,
    },

    /// The prompt was over budget before a call and had to be trimmed.
    /// `remaining_estimate` is the estimated token count *after* this step,
    /// so repeated pressure events show whether trimming is converging.
    BudgetPressure {
        task_id: TaskId,
        step: BudgetStep,
        remaining_estimate: usize,
    },

    /// Older turns were summarized into a single message to free up budget,
    /// after evicting images/text/whole turns wasn't enough on its own.
    TranscriptCompacted {
        task_id: TaskId,
        turns_compacted: usize,
    },

    /// The model tried to end the turn with a mutating tool call still
    /// unverified. The loop nudges it to check first (see
    /// `agent_loop.rs`'s verification gate) — this event marks the one
    /// case where that nudge is skipped and the answer is let through
    /// anyway, so the turn can't get stuck nagging forever.
    AnsweredUnverified {
        task_id: TaskId,
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
