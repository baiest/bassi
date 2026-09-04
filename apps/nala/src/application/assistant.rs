use std::sync::{Arc, Mutex};

// `SystemClock` and `HeuristicTokenCounter` are adapters, but both wrap
// nothing but stdlib calls / plain arithmetic — no I/O, no state, no
// dependency on this process's wiring — so `Assistant::new` can default to
// them directly instead of forcing every caller to thread one through
// their own construction code. Anything that needs a different one (tests,
// mainly) overrides it with `with_clock` / `with_token_counter`.
use crate::adapters::clock::system::SystemClock;
use crate::adapters::memory::in_memory::InMemoryMemoryStore;
use crate::adapters::token_counter::heuristic::HeuristicTokenCounter;
use crate::application::agent_loop::TaskState;
use crate::application::context_budget::ContextBudget;
use crate::application::loop_limits::LoopLimits;
use crate::application::tools::registry::ToolRegistry;
use crate::application::transcript::Transcript;
use crate::ports::autonomous::AutonomousAgent;
use crate::ports::cancellation::{CancelSignal, NeverCancelled};
use crate::ports::clock::Clock;
use crate::ports::events::EventSink;
use crate::ports::llm::{Llm, Message, ToolCall};
use crate::ports::memory::MemoryStore;
use crate::ports::token_counter::TokenCounter;
use crate::ports::tool_dispatcher::{ToolDispatcher, ToolOutcome};

pub(crate) const SYSTEM_PROMPT: &str = "<role>
You are Nala, a general-purpose assistant. You have tools available — including a shell on the user's computer via execute_command, plus whatever other tools are listed below — and you use them to actually accomplish what's asked instead of just describing how it could be done.
</role>

<core_rules>
- ALWAYS use a tool to perform an action the user asked for, when one is available for it. NEVER describe how the user could do it manually if a tool can do it.
- If a tool call fails, do NOT repeat the exact same call. Read the error and change your approach.
- NEVER guess usernames, paths, directories, operating systems, or shells — read them from the computer context provided in each request.
- NEVER answer with a tool's raw output verbatim. Rephrase it as a direct, natural-language answer to what the user asked.
- ALWAYS answer in the same language the user wrote their request in. If they wrote in Spanish, answer in Spanish — never switch to English mid-conversation regardless of what language tool output happens to be in.
- For factual or current-events questions, use web_search (and fetch_url to read a promising result in detail) instead of guessing from memory. Do NOT use execute_command for that.
- The open_url, open_app, and volume capabilities (as open_url/open_app/volume, or prefixed with a connected device's name, e.g. pc_open_url) act directly on a real machine (opening a real URL/app, changing real system volume). Use them directly when the user asks for that action — unlike execute_command, they are narrowly scoped and safe by design, so there's no need to ask for confirmation before calling them. Only one form of each capability is ever offered at a time — call it exactly as listed in your tools, whichever name that is.
- If you aren't sure of the exact name of the app the user wants opened, call the list_apps capability first to look up its real installed name, then pass that confirmed name to open_app — don't guess.
- If the user shares a durable fact about themselves worth keeping for future conversations (their name, a preference, where they live, ...), call remember with a short key and the value. If they later correct a fact you already know, call remember again with the same key — it replaces the old value.
</core_rules>

<verification>
A tool call that changes something comes back with evidence of its effect attached to the result — e.g. a before/after state comparison for a shell command. That evidence is not decorative: look at it before deciding what to do next. \"The call ran\" and \"the task is done\" are different facts — an action can fail silently, target the wrong thing, or have no visible effect. If the attached evidence doesn't show the expected effect, do not proceed or claim success — retry with a different approach instead. If you answer without having genuinely checked the last action's attached evidence, you will be asked to verify before your answer is accepted.
</verification>

<plan_usage>
You will see a Plan message before you start: a short numbered plan you generated for this request. Follow it, but adjust on the fly as tool results come in — it is a starting point, not a script to repeat verbatim if reality does not match it.
</plan_usage>";

pub(crate) const PLANNING_INSTRUCTIONS: &str = "Before doing anything, write a short numbered plan for how you will accomplish the user's request, using the tools listed below and the computer context above. Think about which tool applies, what specific target you need to act on, and how you'll confirm you actually succeeded (not just attempted the first step).

Respond with ONLY the plan as plain numbered text. Do not call any tool yet — this message has no tools available on purpose, so calling one is impossible; just describe the steps.";

pub struct Assistant<L, D, E> {
    // `Arc<Mutex<...>>` rather than a bare `L`: a call to the LLM runs on a
    // background thread (see `agent_loop::call_llm`) so the main thread can
    // abandon waiting on it as soon as cancellation is requested, instead
    // of being stuck until the in-flight HTTP request itself returns.
    pub(crate) llm: Arc<Mutex<L>>,
    pub(crate) dispatcher: D,
    pub(crate) registry: ToolRegistry,
    pub(crate) transcript: Transcript,
    pub(crate) events: E,
    pub(crate) limits: LoopLimits,
    pub(crate) clock: Box<dyn Clock>,
    pub(crate) cancel: Box<dyn CancelSignal>,
    pub(crate) token_counter: Box<dyn TokenCounter>,
    pub(crate) budget: ContextBudget,
    pub(crate) planning_enabled: bool,
    /// Durable facts taught via the `remember` tool, injected fresh into
    /// the prompt each turn (see `agent_loop::build_prompt_messages`) —
    /// separate from `transcript`, which only ever holds conversation text.
    /// Defaults to an in-memory store so a caller that doesn't opt into
    /// persistence (most tests) doesn't need a real file; `bootstrap.rs`
    /// overrides it with a `FileMemoryStore` via `with_memory`.
    pub(crate) memory: Box<dyn MemoryStore>,
    /// Identity and per-call counters for the task currently in
    /// `process()`. Reset at the start of every call.
    pub(crate) current_task: TaskState,
    /// Labels attached to every `LlmStarted`/`LlmCompleted`/`LlmFailed`
    /// event, since `Llm` itself doesn't expose them (Nala wires exactly
    /// one per run). Set via `with_llm_info`; `bootstrap.rs` fills in the
    /// real values.
    pub(crate) provider: String,
    pub(crate) model: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AssistantError<L, D>
where
    L: std::error::Error + 'static,
    D: std::error::Error + 'static,
{
    #[error("LLM error: {0}")]
    Llm(#[source] L),
    #[error("tool error: {0}")]
    Tool(#[source] D),
    #[error("loop detected: the same tool call was requested {0} times in a row")]
    LoopDetected(usize),
    #[error("tool call limit exceeded")]
    ToolCallLimitExceeded,
    #[error("the model produced neither text nor a tool call too many times in a row")]
    EmptyResponse,
    #[error("{0} tool calls in a row failed")]
    TooManyConsecutiveToolFailures(usize),
    #[error("cancelled")]
    Cancelled,
}

impl<L, D, E> Assistant<L, D, E>
where
    L: Llm + Send + 'static,
    D: ToolDispatcher<Output = ToolOutcome>,
    D::Error: std::error::Error + 'static,
    E: EventSink,
{
    pub fn new(llm: L, dispatcher: D, registry: ToolRegistry, events: E) -> Self {
        Self {
            llm: Arc::new(Mutex::new(llm)),
            dispatcher,
            registry,
            events,
            transcript: Transcript::new(system_message(SYSTEM_PROMPT.to_string())),
            limits: LoopLimits::from_env(),
            clock: Box::new(SystemClock::new()),
            cancel: Box::new(NeverCancelled),
            token_counter: Box::new(HeuristicTokenCounter::new()),
            budget: ContextBudget::from_env(),
            planning_enabled: true,
            memory: Box::new(InMemoryMemoryStore::new()),
            current_task: TaskState::default(),
            provider: "unknown".to_string(),
            model: "unknown".to_string(),
        }
    }

    pub fn with_llm_info(mut self, provider: impl Into<String>, model: impl Into<String>) -> Self {
        self.provider = provider.into();
        self.model = model.into();
        self
    }

    pub fn with_planning_enabled(mut self, planning_enabled: bool) -> Self {
        self.planning_enabled = planning_enabled;
        self
    }

    pub fn with_limits(mut self, limits: LoopLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn with_clock(mut self, clock: Box<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    pub fn with_cancel_signal(mut self, cancel: Box<dyn CancelSignal>) -> Self {
        self.cancel = cancel;
        self
    }

    pub fn with_token_counter(mut self, token_counter: Box<dyn TokenCounter>) -> Self {
        self.token_counter = token_counter;
        self
    }

    pub fn with_budget(mut self, budget: ContextBudget) -> Self {
        self.budget = budget;
        self
    }

    pub fn with_memory(mut self, memory: Box<dyn MemoryStore>) -> Self {
        self.memory = memory;
        self
    }

    /// Number of persisted messages, including the system prompt.
    pub fn message_count(&self) -> usize {
        self.transcript.len()
    }

    /// The system prompt's content, if the (always-first) message is still
    /// the system prompt.
    pub fn system_prompt(&self) -> Option<&str> {
        self.transcript.system_prompt()
    }

    /// The event sink, for inspecting what was emitted (tests only).
    pub fn events(&self) -> &E {
        &self.events
    }
}

/// The bridge the autonomous event loop uses to reuse this same agent loop
/// -- see `ports::autonomous::AutonomousAgent`. Every autonomous turn is
/// tagged `RequestSource::Autonomous`, so it's distinguishable from user
/// turns in metrics, and runs through the exact same `process_from` as
/// everything else: no reasoning logic is duplicated for it.
impl<L, D, E> AutonomousAgent for Assistant<L, D, E>
where
    L: Llm + Send + 'static,
    D: ToolDispatcher<Output = ToolOutcome>,
    D::Error: std::error::Error + 'static,
    E: EventSink,
{
    fn respond_to(&mut self, prompt: &str) -> Result<String, String> {
        self.process_from(prompt, crate::ports::events::RequestSource::Autonomous)
            .map_err(|error| error.to_string())
    }
}

pub(crate) fn system_message(content: String) -> Message {
    Message {
        role: "system".to_string(),
        content,
        tool_calls: None,
        tool_name: None,
        images: Vec::new(),
    }
}

pub(crate) fn user_message(content: &str) -> Message {
    Message {
        role: "user".to_string(),
        content: content.to_string(),
        tool_calls: None,
        tool_name: None,
        images: Vec::new(),
    }
}

pub(crate) fn assistant_text_message(content: String) -> Message {
    Message {
        role: "assistant".to_string(),
        content,
        tool_calls: None,
        tool_name: None,
        images: Vec::new(),
    }
}

pub(crate) fn assistant_tool_call_message(tool_call: ToolCall) -> Message {
    Message {
        role: "assistant".to_string(),
        content: String::new(),
        tool_calls: Some(vec![tool_call]),
        tool_name: None,
        images: Vec::new(),
    }
}

pub(crate) fn tool_result_message(tool_name: String, outcome: ToolOutcome) -> Message {
    Message {
        role: "tool".to_string(),
        content: outcome.text,
        tool_calls: None,
        tool_name: Some(tool_name),
        images: outcome.images,
    }
}
