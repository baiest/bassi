use nala::ports::llm::{Llm, LlmError, LlmResponse, Message, ToolCall};
use nala::ports::tool::ToolDefinition;

#[derive(Default)]
pub struct FakeLlm {
    calls: u32,
}

impl FakeLlm {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Llm for FakeLlm {
    fn generate(
        &mut self,
        _messages: &[Message],
        _tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        self.calls += 1;

        if self.calls == 1 {
            return Ok(LlmResponse::ToolCall(ToolCall {
                name: "execute_command".to_string(),
                arguments: r#"{"command":"start chrome"}"#.to_string(),
            }));
        }

        Ok(LlmResponse::Text("chrome opened".to_string()))
    }
}

/// Unique arguments per call avoid tripping loop detection, so this can
/// exercise the MAX_TOOL_CALLS limit on its own.
#[derive(Default)]
pub struct AlwaysCallsToolLlm {
    calls: u32,
}

impl AlwaysCallsToolLlm {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Llm for AlwaysCallsToolLlm {
    fn generate(
        &mut self,
        _messages: &[Message],
        _tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        let calls = self.calls;
        self.calls += 1;

        Ok(LlmResponse::ToolCall(ToolCall {
            name: "execute_command".to_string(),
            arguments: format!(r#"{{"command":"echo {calls}"}}"#),
        }))
    }
}

#[derive(Default)]
pub struct FailingLlm;

impl FailingLlm {
    pub fn new() -> Self {
        Self
    }
}

impl Llm for FailingLlm {
    fn generate(
        &mut self,
        _messages: &[Message],
        _tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        Err(LlmError::RequestFailed("connection refused".to_string()))
    }
}

#[derive(Default)]
pub struct EchoesLastMessageLlm {
    calls: u32,
}

impl EchoesLastMessageLlm {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Llm for EchoesLastMessageLlm {
    fn generate(
        &mut self,
        messages: &[Message],
        _tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        self.calls += 1;

        if self.calls == 1 {
            return Ok(LlmResponse::ToolCall(ToolCall {
                name: "execute_command".to_string(),
                arguments: r#"{"command":"start chrome"}"#.to_string(),
            }));
        }

        let last_content = messages
            .last()
            .map(|message| message.content.clone())
            .unwrap_or_default();

        Ok(LlmResponse::Text(last_content))
    }
}

#[derive(Default)]
pub struct RepeatsSameToolCallLlm;

impl RepeatsSameToolCallLlm {
    pub fn new() -> Self {
        Self
    }
}

impl Llm for RepeatsSameToolCallLlm {
    fn generate(
        &mut self,
        _messages: &[Message],
        _tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        Ok(LlmResponse::ToolCall(ToolCall {
            name: "execute_command".to_string(),
            arguments: r#"{"command":"start chrome"}"#.to_string(),
        }))
    }
}
