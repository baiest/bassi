use std::time::Instant;

use crate::application::tools::registry::ToolRegistry;
use crate::ports::events::{Event, EventSink};
use crate::ports::llm::{Llm, LlmError, LlmResponse, Message, ToolCall};
use crate::ports::tool::ToolDefinition;
use crate::ports::tool_dispatcher::{ToolDispatcher, ToolOutcome};

pub const MAX_TOOL_CALLS: usize = 30;

/// How many times in a row the exact same tool call (name + arguments) can
/// be requested before the turn is aborted as a loop. Multi-step flows
/// (screenshot, click, screenshot again) legitimately call the same tool
/// repeatedly with different arguments — only an identical call repeated
/// back-to-back indicates the model is stuck.
pub const MAX_IDENTICAL_REPEATS: usize = 3;

/// Caps how many messages `self.messages` keeps, so a long-running session
/// doesn't grow the prompt (and its token cost) without bound. The system
/// prompt at index 0 is never counted against this limit or pruned.
pub const MAX_HISTORY_MESSAGES: usize = 20;

const SYSTEM_PROMPT: &str = "<role>
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

const PLANNING_INSTRUCTIONS: &str = "Before doing anything, write a short numbered plan for how you will accomplish the user's request, using the tools listed below and the computer context above. Think about which application or tool applies, whether you need to search for something, what specific target you need to find and act on, and how you'll confirm you actually succeeded (not just attempted the first step).

The `type` tool types into whatever currently has keyboard focus — it does not find or focus a field for you. Before any step that types into a specific field (a search bar, a text box, a URL bar), the plan must include a separate click step on that field first, using a screenshot to locate it. Never plan to type immediately after opening or navigating to a page.

Respond with ONLY the plan as plain numbered text. Do not call any tool yet — this message has no tools available on purpose, so calling one is impossible; just describe the steps.";

pub struct Assistant<L, D, E> {
    llm: L,
    dispatcher: D,
    registry: ToolRegistry,
    messages: Vec<Message>,
    events: E,
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
    #[error(
        "loop detected: the same tool call was requested {MAX_IDENTICAL_REPEATS} times in a row"
    )]
    LoopDetected,
    #[error("tool call limit exceeded")]
    ToolCallLimitExceeded,
}

impl<L, D, E> Assistant<L, D, E>
where
    L: Llm,
    D: ToolDispatcher<Output = ToolOutcome>,
    D::Error: std::error::Error + 'static,
    E: EventSink,
{
    pub fn new(llm: L, dispatcher: D, registry: ToolRegistry, events: E) -> Self {
        Self {
            llm,
            dispatcher,
            registry,
            events,
            messages: vec![system_message(SYSTEM_PROMPT.to_string())],
        }
    }

    /// Number of persisted messages, including the system prompt.
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// The system prompt's content, if the (always-first) message is still
    /// the system prompt.
    pub fn system_prompt(&self) -> Option<&str> {
        self.messages
            .first()
            .filter(|message| message.role == "system")
            .map(|message| message.content.as_str())
    }

    /// The event sink, for inspecting what was emitted (tests only).
    pub fn events(&self) -> &E {
        &self.events
    }

    pub fn process(&mut self, input: &str) -> Result<String, AssistantError<LlmError, D::Error>> {
        let request_start = Instant::now();

        self.events.emit(Event::RequestStarted);

        self.push_history(user_message(input));

        let mut messages = self.build_prompt_messages()?;
        // Cloned rather than borrowed from `self.registry`, so `tools` doesn't
        // keep an immutable borrow of `self` alive across the `&mut self`
        // calls below (`build_plan`, `self.llm.generate`, ...).
        let tool_definitions: Vec<ToolDefinition> =
            self.registry.definitions().into_iter().cloned().collect();
        let tools: Vec<&ToolDefinition> = tool_definitions.iter().collect();

        if let Some(plan) = self.build_plan(&messages, &tools) {
            self.events.emit(Event::PlanCreated { plan: plan.clone() });
            messages.push(assistant_text_message(format!("Plan:\n{plan}")));
        }

        let mut last_tool_call: Option<ToolCall> = None;
        let mut identical_repeats: usize = 0;
        let mut tool_call_count: usize = 0;

        loop {
            let outgoing_images: usize = messages.iter().map(|message| message.images.len()).sum();
            self.events.emit(Event::LlmStarted {
                images: outgoing_images,
            });

            let start = Instant::now();
            let response = self.llm.generate(&messages, &tools);

            let duration = start.elapsed();

            let response = match response {
                Ok(response) => response,
                Err(error) => {
                    let duration = request_start.elapsed();

                    self.events.emit(Event::RequestFailed {
                        duration,
                        error: error.to_string(),
                    });

                    return Err(AssistantError::Llm(error));
                }
            };

            self.events.emit(Event::LlmCompleted { duration });

            match response {
                LlmResponse::ToolCall(tool_call) => {
                    messages.push(assistant_tool_call_message(tool_call.clone()));

                    if last_tool_call.as_ref() == Some(&tool_call) {
                        identical_repeats += 1;
                    } else {
                        identical_repeats = 1;
                        last_tool_call = Some(tool_call.clone());
                    }

                    if identical_repeats >= MAX_IDENTICAL_REPEATS {
                        let duration = request_start.elapsed();

                        self.events.emit(Event::RequestFailed {
                            duration,
                            error: "loop detected".to_string(),
                        });

                        return Err(AssistantError::LoopDetected);
                    }
                    tool_call_count += 1;

                    let tool_name = tool_call.name.clone();
                    let tool_args = tool_call.arguments.clone();

                    self.events.emit(Event::ToolStarted {
                        name: tool_name.clone(),
                        arguments: tool_args,
                    });

                    let start = Instant::now();

                    let outcome = handle_tool_call(&mut self.dispatcher, tool_call);

                    let duration = start.elapsed();

                    self.events.emit(Event::ToolCompleted {
                        name: tool_name.clone(),
                        duration,
                        output: outcome.text.clone(),
                        images: outcome.images.len(),
                    });

                    messages.push(tool_result_message(tool_name, outcome));

                    if tool_call_count >= MAX_TOOL_CALLS {
                        let duration = request_start.elapsed();

                        self.events.emit(Event::RequestFailed {
                            duration,
                            error: "tool call limit exceeded".to_string(),
                        });

                        return Err(AssistantError::ToolCallLimitExceeded);
                    }
                }
                LlmResponse::Text(text) => {
                    self.push_history(assistant_text_message(text.clone()));

                    let duration = request_start.elapsed();

                    self.events.emit(Event::RequestCompleted { duration });

                    break Ok(text);
                }
            }
        }
    }

    /// Asks the model for a short step-by-step plan before it does anything,
    /// so a multi-step task (open an app, find a specific item, act on it)
    /// has an explicit target to follow instead of being decided one tool
    /// call at a time with no larger goal in view.
    ///
    /// Tools are deliberately withheld from this call (an empty slice, not
    /// `tools`) — a text-summarized list is included in the prompt instead
    /// — so a tool-eager model can't skip straight to acting instead of
    /// planning. A failure here (network error, etc.) isn't fatal to the
    /// request: it just means proceeding without a plan.
    fn build_plan(&mut self, messages: &[Message], tools: &[&ToolDefinition]) -> Option<String> {
        let tool_summary: String = tools
            .iter()
            .map(|tool| format!("- {}: {}", tool.name, tool.description))
            .collect::<Vec<_>>()
            .join("\n");

        let mut planning_messages = messages.to_vec();
        planning_messages.push(system_message(format!(
            "Available tools:\n{tool_summary}\n\n{PLANNING_INSTRUCTIONS}"
        )));

        self.events.emit(Event::LlmStarted { images: 0 });
        let start = Instant::now();
        let response = self.llm.generate(&planning_messages, &[]);
        let duration = start.elapsed();

        let response = response.ok()?;
        self.events.emit(Event::LlmCompleted { duration });

        match response {
            LlmResponse::Text(plan) if !plan.trim().is_empty() => Some(plan),
            _ => None,
        }
    }

    /// Context can change between turns, so it is fetched fresh each time
    /// and never persisted in `self.messages`.
    fn build_prompt_messages(
        &mut self,
    ) -> Result<Vec<Message>, AssistantError<LlmError, D::Error>> {
        let context = self
            .dispatcher
            .get_context()
            .map_err(AssistantError::Tool)?;

        let mut messages = self.messages.clone();
        messages.push(system_message(format!(
            "Computer context:\n{context}\n\nUse this context when generating commands."
        )));

        Ok(messages)
    }

    /// Appends to the persisted history, then drops the oldest non-system
    /// messages until it's back within `MAX_HISTORY_MESSAGES`.
    fn push_history(&mut self, message: Message) {
        self.messages.push(message);

        while self.messages.len() > MAX_HISTORY_MESSAGES {
            self.messages.remove(1);
        }
    }
}

fn handle_tool_call<D>(dispatcher: &mut D, tool_call: ToolCall) -> ToolOutcome
where
    D: ToolDispatcher<Output = ToolOutcome>,
    D::Error: std::error::Error + 'static,
{
    match dispatcher.dispatch(tool_call) {
        Ok(outcome) => outcome,
        Err(error) => ToolOutcome::from(format!("ERROR: {error}")),
    }
}

fn system_message(content: String) -> Message {
    Message {
        role: "system".to_string(),
        content,
        tool_calls: None,
        tool_name: None,
        images: Vec::new(),
    }
}

fn user_message(content: &str) -> Message {
    Message {
        role: "user".to_string(),
        content: content.to_string(),
        tool_calls: None,
        tool_name: None,
        images: Vec::new(),
    }
}

fn assistant_text_message(content: String) -> Message {
    Message {
        role: "assistant".to_string(),
        content,
        tool_calls: None,
        tool_name: None,
        images: Vec::new(),
    }
}

fn assistant_tool_call_message(tool_call: ToolCall) -> Message {
    Message {
        role: "assistant".to_string(),
        content: String::new(),
        tool_calls: Some(vec![tool_call]),
        tool_name: None,
        images: Vec::new(),
    }
}

fn tool_result_message(tool_name: String, outcome: ToolOutcome) -> Message {
    Message {
        role: "tool".to_string(),
        content: outcome.text,
        tool_calls: None,
        tool_name: Some(tool_name),
        images: outcome.images,
    }
}
