use std::cell::RefCell;
use std::rc::Rc;

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
        tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        if tools.is_empty() {
            return Ok(LlmResponse::Text("plan".to_string()));
        }

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

/// Targets a tool name that isn't registered, so every call fails. Combined
/// with unique arguments per call, this avoids tripping loop detection, so
/// it can exercise the MAX_TOOL_CALLS limit on its own.
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
            name: "unregistered_tool".to_string(),
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
        tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        if tools.is_empty() {
            return Ok(LlmResponse::Text("plan".to_string()));
        }

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

/// Always replies with text immediately, never requesting a tool call. Used
/// to run many turns in a row without tripping the tool-call machinery.
#[derive(Default)]
pub struct AlwaysRepliesTextLlm;

impl AlwaysRepliesTextLlm {
    pub fn new() -> Self {
        Self
    }
}

impl Llm for AlwaysRepliesTextLlm {
    fn generate(
        &mut self,
        _messages: &[Message],
        _tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        Ok(LlmResponse::Text("ok".to_string()))
    }
}

#[derive(Default)]
pub struct ResolvesInOneToolCallLlm {
    calls: u32,
}

impl ResolvesInOneToolCallLlm {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Llm for ResolvesInOneToolCallLlm {
    fn generate(
        &mut self,
        _messages: &[Message],
        tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        if tools.is_empty() {
            return Ok(LlmResponse::Text("plan".to_string()));
        }

        self.calls += 1;

        if self.calls == 1 {
            return Ok(LlmResponse::ToolCall(ToolCall {
                name: "execute_command".to_string(),
                arguments: r#"{"command":"date"}"#.to_string(),
            }));
        }

        Ok(LlmResponse::Text("it's 10:00 AM".to_string()))
    }
}

#[derive(Default)]
pub struct RetriesSameToolWithDifferentArgsLlm {
    calls: u32,
}

impl RetriesSameToolWithDifferentArgsLlm {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Llm for RetriesSameToolWithDifferentArgsLlm {
    fn generate(
        &mut self,
        _messages: &[Message],
        _tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        let calls = self.calls;
        self.calls += 1;

        Ok(LlmResponse::ToolCall(ToolCall {
            name: "execute_command".to_string(),
            arguments: format!(r#"{{"command":"date-variant-{calls}"}}"#),
        }))
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

/// Models a multi-step flow (e.g. screenshot -> click -> screenshot):
/// requests two *distinct* tool calls, then answers with text. Since no
/// call repeats identically, this must resolve without tripping
/// `LoopDetected`.
#[derive(Default)]
pub struct ChainsDistinctToolCallsThenAnswersLlm {
    calls: u32,
}

impl ChainsDistinctToolCallsThenAnswersLlm {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Llm for ChainsDistinctToolCallsThenAnswersLlm {
    fn generate(
        &mut self,
        _messages: &[Message],
        tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        if tools.is_empty() {
            return Ok(LlmResponse::Text("plan".to_string()));
        }

        let calls = self.calls;
        self.calls += 1;

        match calls {
            0 | 1 => Ok(LlmResponse::ToolCall(ToolCall {
                name: "execute_command".to_string(),
                arguments: format!(r#"{{"command":"date-step-{calls}"}}"#),
            })),
            _ => Ok(LlmResponse::Text("done".to_string())),
        }
    }
}

/// Calls `screenshot` once, then answers with text. Used to exercise the
/// path where a tool result carries images.
#[derive(Default)]
pub struct CallsScreenshotThenAnswersLlm {
    calls: u32,
}

impl CallsScreenshotThenAnswersLlm {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Llm for CallsScreenshotThenAnswersLlm {
    fn generate(
        &mut self,
        _messages: &[Message],
        tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        if tools.is_empty() {
            return Ok(LlmResponse::Text("plan".to_string()));
        }

        let calls = self.calls;
        self.calls += 1;

        match calls {
            0 => Ok(LlmResponse::ToolCall(ToolCall {
                name: "screenshot".to_string(),
                arguments: "{}".to_string(),
            })),
            _ => Ok(LlmResponse::Text("done".to_string())),
        }
    }
}

/// Requests the exact same tool call twice, then answers with text. Two
/// identical repeats is below `MAX_IDENTICAL_REPEATS`, so this must
/// resolve normally rather than triggering `LoopDetected`.
#[derive(Default)]
pub struct RepeatsSameCallTwiceThenAnswersLlm {
    calls: u32,
}

impl RepeatsSameCallTwiceThenAnswersLlm {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Llm for RepeatsSameCallTwiceThenAnswersLlm {
    fn generate(
        &mut self,
        _messages: &[Message],
        tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        if tools.is_empty() {
            return Ok(LlmResponse::Text("plan".to_string()));
        }

        let calls = self.calls;
        self.calls += 1;

        match calls {
            0 | 1 => Ok(LlmResponse::ToolCall(ToolCall {
                name: "execute_command".to_string(),
                arguments: r#"{"command":"date"}"#.to_string(),
            })),
            _ => Ok(LlmResponse::Text("done".to_string())),
        }
    }
}

/// First call returns a plain-text plan; second call (the first that can
/// actually call a tool) is recorded into `messages_on_execute_call` so a
/// test can assert the plan text made it into context; then it calls a
/// tool and finally answers with text.
#[derive(Default)]
pub struct PlansThenExecutesLlm {
    calls: u32,
    pub plan: String,
    pub messages_on_execute_call: Rc<RefCell<Option<Vec<Message>>>>,
}

impl PlansThenExecutesLlm {
    pub fn new(plan: &str) -> Self {
        Self {
            calls: 0,
            plan: plan.to_string(),
            messages_on_execute_call: Rc::new(RefCell::new(None)),
        }
    }
}

impl Llm for PlansThenExecutesLlm {
    fn generate(
        &mut self,
        messages: &[Message],
        _tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        let calls = self.calls;
        self.calls += 1;

        match calls {
            0 => Ok(LlmResponse::Text(self.plan.clone())),
            1 => {
                *self.messages_on_execute_call.borrow_mut() = Some(messages.to_vec());
                Ok(LlmResponse::ToolCall(ToolCall {
                    name: "execute_command".to_string(),
                    arguments: r#"{"command":"start spotify"}"#.to_string(),
                }))
            }
            _ => Ok(LlmResponse::Text("done".to_string())),
        }
    }
}

/// Fails only the first (planning) call, then behaves normally — used to
/// verify a failed planning step doesn't abort the whole request.
#[derive(Default)]
pub struct FailsPlanningThenExecutesLlm {
    calls: u32,
}

impl FailsPlanningThenExecutesLlm {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Llm for FailsPlanningThenExecutesLlm {
    fn generate(
        &mut self,
        _messages: &[Message],
        _tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        let calls = self.calls;
        self.calls += 1;

        match calls {
            0 => Err(LlmError::RequestFailed("connection refused".to_string())),
            1 => Ok(LlmResponse::ToolCall(ToolCall {
                name: "execute_command".to_string(),
                arguments: r#"{"command":"start spotify"}"#.to_string(),
            })),
            _ => Ok(LlmResponse::Text("done".to_string())),
        }
    }
}
