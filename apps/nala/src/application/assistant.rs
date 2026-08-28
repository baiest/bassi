use crate::application::tools::registry::ToolRegistry;
use crate::ports::llm::{Llm, LlmError, LlmResponse, Message, ToolCall};
use crate::ports::tool_dispatcher::ToolDispatcher;

pub const MAX_TOOL_CALLS: usize = 10;

const SYSTEM_PROMPT: &str = "You are Nala, a computer assistant.

When the user asks you to perform an action, use the available tools.

Do not explain how the user can perform the action manually when you can perform it yourself.

If a tool execution fails, use the error information to decide what to do next.

When the task is completed, briefly tell the user what was done.

Always use the provided computer context when generating commands.

Do not guess usernames, paths, directories, operating systems, or shells.";

pub struct Assistant<L, D> {
    llm: L,
    dispatcher: D,
    registry: ToolRegistry,
    messages: Vec<Message>,
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
    #[error("loop detected: the same tool call was requested twice in a row")]
    LoopDetected,
    #[error("tool call limit exceeded")]
    ToolCallLimitExceeded,
}

impl<L, D> Assistant<L, D>
where
    L: Llm,
    D: ToolDispatcher<Output = String>,
    D::Error: std::error::Error + 'static,
{
    pub fn new(llm: L, dispatcher: D, registry: ToolRegistry) -> Self {
        Self {
            llm,
            dispatcher,
            registry,
            messages: vec![system_message(SYSTEM_PROMPT.to_string())],
        }
    }

    pub fn process(
        &mut self,
        input: &str,
    ) -> Result<D::Output, AssistantError<LlmError, D::Error>> {
        self.messages.push(user_message(input));

        let mut messages = self.build_prompt_messages()?;
        let tools = self.registry.definitions();

        let mut executed_tools: Vec<ToolCall> = Vec::new();
        let mut tool_call_count: usize = 0;

        loop {
            let response = self
                .llm
                .generate(&messages, &tools)
                .map_err(AssistantError::Llm)?;

            match response {
                LlmResponse::ToolCall(tool_call) => {
                    messages.push(assistant_tool_call_message(tool_call.clone()));

                    if executed_tools.contains(&tool_call) {
                        return Err(AssistantError::LoopDetected);
                    }
                    executed_tools.push(tool_call.clone());
                    tool_call_count += 1;

                    let output = handle_tool_call(&mut self.dispatcher, tool_call);
                    messages.push(tool_result_message(output));

                    if tool_call_count >= MAX_TOOL_CALLS {
                        return Err(AssistantError::ToolCallLimitExceeded);
                    }
                }
                LlmResponse::Text(text) => {
                    self.messages.push(assistant_text_message(text.clone()));

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
}

fn handle_tool_call<D>(dispatcher: &mut D, tool_call: ToolCall) -> String
where
    D: ToolDispatcher<Output = String>,
    D::Error: std::error::Error + 'static,
{
    match dispatcher.dispatch(tool_call) {
        Ok(output) => output,
        Err(error) => format!("ERROR: {error}"),
    }
}

fn system_message(content: String) -> Message {
    Message {
        role: "system".to_string(),
        content,
        tool_calls: None,
    }
}

fn user_message(content: &str) -> Message {
    Message {
        role: "user".to_string(),
        content: content.to_string(),
        tool_calls: None,
    }
}

fn assistant_text_message(content: String) -> Message {
    Message {
        role: "assistant".to_string(),
        content,
        tool_calls: None,
    }
}

fn assistant_tool_call_message(tool_call: ToolCall) -> Message {
    Message {
        role: "assistant".to_string(),
        content: String::new(),
        tool_calls: Some(vec![tool_call]),
    }
}

fn tool_result_message(content: String) -> Message {
    Message {
        role: "tool".to_string(),
        content,
        tool_calls: None,
    }
}
