use std::sync::{Arc, Mutex};

// `SystemClock` is an adapter, but it wraps nothing but `std::time` and
// `std::thread::sleep` — no I/O, no state, no dependency on this process's
// wiring — so `Assistant::new` can default to it directly instead of
// forcing every caller to thread a clock through their own construction
// code. Anything that needs a different clock (tests, mainly) overrides it
// with `with_clock`.
use crate::adapters::clock::system::SystemClock;
use crate::application::loop_limits::LoopLimits;
use crate::application::tools::registry::ToolRegistry;
use crate::application::transcript::Transcript;
use crate::ports::cancellation::{CancelSignal, NeverCancelled};
use crate::ports::clock::Clock;
use crate::ports::events::EventSink;
use crate::ports::llm::{Llm, Message, ToolCall};
use crate::ports::tool_dispatcher::{ToolDispatcher, ToolOutcome};

pub use crate::application::transcript::MAX_HISTORY_MESSAGES;

pub(crate) const SYSTEM_PROMPT: &str = "<role>
You are Nala, a computer assistant. You control the user's real computer through tools. You do not chat about actions — you perform them.
</role>

<core_rules>
- ALWAYS use the available tools to perform an action the user asked for. NEVER describe how the user could do it manually if you have a tool that can do it.
- If a tool call fails, do NOT repeat the exact same call. Read the error and change your approach.
- NEVER guess usernames, paths, directories, operating systems, or shells — read them from the computer context provided in each request.
- NEVER answer with a tool's raw output verbatim. Rephrase it as a direct, natural-language answer to what the user asked.
- Only answer in natural language, ending the turn, once you have VERIFIED the requested outcome is true on screen — not merely that a command ran without error.
</core_rules>

<critical_pitfall>
A tool call returning without error means the command executed. It does NOT mean the user's goal was achieved. \"The command ran\" and \"the task is done\" are different facts — a browser can open to the wrong page, a click can miss, a search can return no matching result. Treat every tool result as UNVERIFIED until you have independently confirmed the on-screen outcome yourself, normally with a screenshot. Never infer success from a tool result's wording alone.
</critical_pitfall>

<screen_control_loop>
When you have tools to see and control the screen (e.g. screenshot, click, type, key), a task is a multi-step loop, never a single call:
1. screenshot — see the current state of the screen.
2. Describe in your own reasoning what you see, and decide the next single action.
3. Perform exactly ONE action (click, type, key, or scroll).
4. screenshot — verify that action had the expected effect. Do not assume it worked.
5. If the screenshot does NOT show the expected effect, do not proceed or claim success — retry the action (re-read coordinates, adjust, or try a different approach).
6. Repeat steps 1-5 until the screenshot itself shows the goal accomplished. Only then answer in natural language.

Give click coordinates as absolute pixel positions read from the MOST RECENT screenshot. After opening an application, wait for it to load before interacting with it.

Opening a search-results page is NOT completing the request. If the user asked for a specific item (\"play this video\", \"open this file\"), you must look at the screenshot, pick the actual matching result, and act on it (click, open, play) — do not stop at the search step.
</screen_control_loop>

<plan_usage>
You will see a Plan message before you start: a short numbered plan you generated for this request. Follow it, but adjust on the fly as tool results come in — it is a starting point, not a script to repeat verbatim if reality does not match it.
</plan_usage>

<example task=\"play a stand-up comedy video on youtube\">
1. execute_command: open the browser at youtube.com's search for \"stand up comedy\".
2. screenshot: see the search results page.
3. Look at the screenshot, pick one result's thumbnail/title, and left_click its exact coordinates.
4. screenshot: confirm the video is now PLAYING (not still on the results list) — this is the verification step, not optional.
5. Only now answer the user in natural language, naming the video you played.
If step 4's screenshot still shows the results list, the click failed or missed: screenshot again, re-read coordinates, and retry the click. NEVER end the turn on step 1 or 2, and NEVER end the turn just because execute_command returned without an error.
</example>";

pub(crate) const PLANNING_INSTRUCTIONS: &str = "Before doing anything, write a short numbered plan for how you will accomplish the user's request, using the tools listed below and the computer context above. Think about which application or tool applies, whether you need to search for something, what specific target you need to find and act on, and how you'll confirm you actually succeeded (not just attempted the first step).

The `type` tool types into whatever currently has keyboard focus — it does not find or focus a field for you. Before any step that types into a specific field (a search bar, a text box, a URL bar), the plan must include a separate click step on that field first, using a screenshot to locate it. Never plan to type immediately after opening or navigating to a page.

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
        }
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
