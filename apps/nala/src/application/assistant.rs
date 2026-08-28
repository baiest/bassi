use std::time::Instant;

use crate::application::tools::registry::ToolRegistry;
use crate::ports::events::{Event, EventSink};
use crate::ports::llm::{Llm, LlmError, LlmResponse, Message, ToolCall};
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

const SYSTEM_PROMPT: &str = "You are Nala, a computer assistant.

When the user asks you to perform an action, use the available tools.

If a tool execution fails, do not repeat the exact same tool call. Use the error information to change your approach.

Do not explain how the user can perform the action manually when you can perform it yourself.

When the task is completed, briefly tell the user what was done in natural language. Never answer with the raw output of a tool call verbatim; rephrase it as a direct answer to what the user asked.

Always use the provided computer context when generating commands.

Do not guess usernames, paths, directories, operating systems, or shells.

When you have tools to see and control the screen (e.g. screenshot, click, type, key), completing a task is a multi-step loop, not a single call:
1. Take a screenshot before acting, to see the current state of the screen.
2. Describe what you see and decide the next single action.
3. Perform that one action (click, type, key, scroll).
4. Take another screenshot to verify the action had the expected effect before continuing.
5. Repeat until the task is done, then answer in natural language.
Give click coordinates as absolute pixel positions based on the most recent screenshot. After opening an application, wait for it to load before interacting with it.";

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

    pub fn process(&mut self, input: &str) -> Result<String, AssistantError<LlmError, D::Error>> {
        let request_start = Instant::now();

        self.events.emit(Event::RequestStarted);

        self.push_history(user_message(input));

        let mut messages = self.build_prompt_messages()?;
        let tools = self.registry.definitions();

        let mut last_tool_call: Option<ToolCall> = None;
        let mut identical_repeats: usize = 0;
        let mut tool_call_count: usize = 0;

        loop {
            self.events.emit(Event::LlmStarted);

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
