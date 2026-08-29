use std::sync::{Arc, Mutex};

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
            return Ok(LlmResponse::text("plan".to_string()));
        }

        self.calls += 1;

        if self.calls == 1 {
            return Ok(LlmResponse::tool_call(ToolCall {
                name: "execute_command".to_string(),
                arguments: r#"{"command":"start chrome"}"#.to_string(),
            }));
        }

        Ok(LlmResponse::text("chrome opened".to_string()))
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

        Ok(LlmResponse::tool_call(ToolCall {
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

/// Fails the first two calls with a retryable error, then succeeds with
/// text. Used to verify the loop retries a retryable LLM failure instead of
/// aborting the turn immediately.
#[derive(Default)]
pub struct FailsTwiceThenSucceedsLlm {
    calls: u32,
}

impl FailsTwiceThenSucceedsLlm {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Llm for FailsTwiceThenSucceedsLlm {
    fn generate(
        &mut self,
        _messages: &[Message],
        tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        if tools.is_empty() {
            return Ok(LlmResponse::text("plan"));
        }

        let calls = self.calls;
        self.calls += 1;

        if calls < 2 {
            return Err(LlmError::RequestFailed("connection refused".to_string()));
        }

        Ok(LlmResponse::text("recovered"))
    }
}

/// Always fails with a retryable error. Used to verify the loop gives up
/// after `max_llm_retries` instead of retrying forever.
#[derive(Default)]
pub struct AlwaysFailsRetryableLlm;

impl AlwaysFailsRetryableLlm {
    pub fn new() -> Self {
        Self
    }
}

impl Llm for AlwaysFailsRetryableLlm {
    fn generate(
        &mut self,
        _messages: &[Message],
        _tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        Err(LlmError::RequestFailed("connection refused".to_string()))
    }
}

/// Fails with a non-retryable error (a response that doesn't parse). Used
/// to verify the loop doesn't waste retries on a failure retrying can't fix.
#[derive(Default)]
pub struct FailsWithInvalidResponseLlm;

impl FailsWithInvalidResponseLlm {
    pub fn new() -> Self {
        Self
    }
}

impl Llm for FailsWithInvalidResponseLlm {
    fn generate(
        &mut self,
        _messages: &[Message],
        _tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        Err(LlmError::InvalidResponse("malformed body".to_string()))
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
            return Ok(LlmResponse::text("plan".to_string()));
        }

        self.calls += 1;

        if self.calls == 1 {
            return Ok(LlmResponse::tool_call(ToolCall {
                name: "execute_command".to_string(),
                arguments: r#"{"command":"start chrome"}"#.to_string(),
            }));
        }

        let last_content = messages
            .last()
            .map(|message| message.content.clone())
            .unwrap_or_default();

        Ok(LlmResponse::text(last_content))
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
        Ok(LlmResponse::text("ok".to_string()))
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
            return Ok(LlmResponse::text("plan".to_string()));
        }

        self.calls += 1;

        if self.calls == 1 {
            return Ok(LlmResponse::tool_call(ToolCall {
                name: "execute_command".to_string(),
                arguments: r#"{"command":"date"}"#.to_string(),
            }));
        }

        Ok(LlmResponse::text("it's 10:00 AM".to_string()))
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

        Ok(LlmResponse::tool_call(ToolCall {
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
        Ok(LlmResponse::tool_call(ToolCall {
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
            return Ok(LlmResponse::text("plan".to_string()));
        }

        let calls = self.calls;
        self.calls += 1;

        match calls {
            0 | 1 => Ok(LlmResponse::tool_call(ToolCall {
                name: "execute_command".to_string(),
                arguments: format!(r#"{{"command":"date-step-{calls}"}}"#),
            })),
            _ => Ok(LlmResponse::text("done".to_string())),
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
            return Ok(LlmResponse::text("plan".to_string()));
        }

        let calls = self.calls;
        self.calls += 1;

        match calls {
            0 => Ok(LlmResponse::tool_call(ToolCall {
                name: "screenshot".to_string(),
                arguments: "{}".to_string(),
            })),
            _ => Ok(LlmResponse::text("done".to_string())),
        }
    }
}

/// Requests five `screenshot` calls in a single response, then answers with
/// text. All five tool results (and their images) land in the turn's
/// messages before the loop re-checks the budget, so — unlike spreading
/// them across separate round-trips, where eviction can correct each
/// image back down before the next one arrives — this reliably gives a
/// small `keep_recent_images` budget more images than it allows in one
/// shot, for eviction to actually act on.
#[derive(Default)]
pub struct CallsScreenshotFiveTimesThenAnswersLlm {
    calls: u32,
}

impl CallsScreenshotFiveTimesThenAnswersLlm {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Llm for CallsScreenshotFiveTimesThenAnswersLlm {
    fn generate(
        &mut self,
        _messages: &[Message],
        tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        if tools.is_empty() {
            return Ok(LlmResponse::text("plan"));
        }

        let calls = self.calls;
        self.calls += 1;

        if calls == 0 {
            // Distinct arguments per call so this doesn't trip loop
            // detection (identical calls repeated back to back) — a real
            // screenshot-heavy flow wouldn't send identical requests either.
            let tool_calls = (0..5)
                .map(|step| ToolCall {
                    name: "screenshot".to_string(),
                    arguments: format!(r#"{{"step":{step}}}"#),
                })
                .collect();
            Ok(LlmResponse::tool_calls(tool_calls))
        } else {
            Ok(LlmResponse::text("done"))
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
            return Ok(LlmResponse::text("plan".to_string()));
        }

        let calls = self.calls;
        self.calls += 1;

        match calls {
            0 | 1 => Ok(LlmResponse::tool_call(ToolCall {
                name: "execute_command".to_string(),
                arguments: r#"{"command":"date"}"#.to_string(),
            })),
            _ => Ok(LlmResponse::text("done".to_string())),
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
    pub messages_on_execute_call: Arc<Mutex<Option<Vec<Message>>>>,
}

impl PlansThenExecutesLlm {
    pub fn new(plan: &str) -> Self {
        Self {
            calls: 0,
            plan: plan.to_string(),
            messages_on_execute_call: Arc::new(Mutex::new(None)),
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
            0 => Ok(LlmResponse::text(self.plan.clone())),
            1 => {
                *self.messages_on_execute_call.lock().unwrap() = Some(messages.to_vec());
                Ok(LlmResponse::tool_call(ToolCall {
                    name: "execute_command".to_string(),
                    arguments: r#"{"command":"start spotify"}"#.to_string(),
                }))
            }
            _ => Ok(LlmResponse::text("done".to_string())),
        }
    }
}

/// Requests two distinct tool calls in a single response, then answers with
/// text. Used to verify the loop executes every tool call in one LLM
/// round-trip instead of only the first.
#[derive(Default)]
pub struct RequestsTwoToolCallsAtOnceThenAnswersLlm {
    calls: u32,
}

impl RequestsTwoToolCallsAtOnceThenAnswersLlm {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Llm for RequestsTwoToolCallsAtOnceThenAnswersLlm {
    fn generate(
        &mut self,
        _messages: &[Message],
        tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        if tools.is_empty() {
            return Ok(LlmResponse::text("plan"));
        }

        let calls = self.calls;
        self.calls += 1;

        match calls {
            0 => Ok(LlmResponse::tool_calls(vec![
                ToolCall {
                    name: "execute_command".to_string(),
                    arguments: r#"{"command":"date-a"}"#.to_string(),
                },
                ToolCall {
                    name: "execute_command".to_string(),
                    arguments: r#"{"command":"date-b"}"#.to_string(),
                },
            ])),
            _ => Ok(LlmResponse::text("done")),
        }
    }
}

/// Answers with empty text twice (no tool calls), then with real text. Used
/// to verify the loop nudges the model instead of ending the turn on an
/// empty response, but still terminates instead of looping forever.
#[derive(Default)]
pub struct AnswersEmptyTwiceThenTextLlm {
    calls: u32,
}

impl AnswersEmptyTwiceThenTextLlm {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Llm for AnswersEmptyTwiceThenTextLlm {
    fn generate(
        &mut self,
        _messages: &[Message],
        tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        if tools.is_empty() {
            return Ok(LlmResponse::text("plan"));
        }

        let calls = self.calls;
        self.calls += 1;

        match calls {
            0 | 1 => Ok(LlmResponse::default()),
            _ => Ok(LlmResponse::text("done")),
        }
    }
}

/// Always answers with empty text and no tool call. Used to verify the loop
/// eventually gives up instead of looping forever.
#[derive(Default)]
pub struct AlwaysAnswersEmptyLlm;

impl AlwaysAnswersEmptyLlm {
    pub fn new() -> Self {
        Self
    }
}

impl Llm for AlwaysAnswersEmptyLlm {
    fn generate(
        &mut self,
        _messages: &[Message],
        _tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        Ok(LlmResponse::default())
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
            1 => Ok(LlmResponse::tool_call(ToolCall {
                name: "execute_command".to_string(),
                arguments: r#"{"command":"start spotify"}"#.to_string(),
            })),
            _ => Ok(LlmResponse::text("done".to_string())),
        }
    }
}

/// Answers the planning call immediately (so a turn reaches the main loop),
/// then blocks forever on the first call that has tools available — it
/// never returns. Used to prove that cancelling mid-call abandons the wait
/// instead of blocking until the (never-arriving) result, the exact bug
/// this fake exists to catch: cancellation must interrupt an in-flight LLM
/// call, not just the gaps between calls.
#[derive(Default)]
pub struct HangsOnRealCallLlm;

impl HangsOnRealCallLlm {
    pub fn new() -> Self {
        Self
    }
}

impl Llm for HangsOnRealCallLlm {
    fn generate(
        &mut self,
        _messages: &[Message],
        tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        if tools.is_empty() {
            return Ok(LlmResponse::text("plan"));
        }

        loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
    }
}

/// Calls `execute_command` (a mutating tool) once, then answers with text
/// immediately — never checking the result. Used to exercise the
/// verification gate: the first such answer should be held back with a
/// nudge, the second let through.
#[derive(Default)]
pub struct MutatesThenAnswersImmediatelyLlm {
    calls: u32,
}

impl MutatesThenAnswersImmediatelyLlm {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Llm for MutatesThenAnswersImmediatelyLlm {
    fn generate(
        &mut self,
        _messages: &[Message],
        tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        if tools.is_empty() {
            return Ok(LlmResponse::text("plan"));
        }

        let calls = self.calls;
        self.calls += 1;

        match calls {
            0 => Ok(LlmResponse::tool_call(ToolCall {
                name: "execute_command".to_string(),
                arguments: r#"{"command":"mkdir test"}"#.to_string(),
            })),
            _ => Ok(LlmResponse::text("done")),
        }
    }
}

/// Calls `execute_command` (mutating), then `ping` (read-only, but still a
/// tool call the model made to check its own work), then answers with
/// text. Used to prove the gate only fires when the *most recent* action
/// was an unverified mutation — any tool call after it, not necessarily a
/// screenshot, clears that.
#[derive(Default)]
pub struct MutatesThenChecksThenAnswersLlm {
    calls: u32,
}

impl MutatesThenChecksThenAnswersLlm {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Llm for MutatesThenChecksThenAnswersLlm {
    fn generate(
        &mut self,
        _messages: &[Message],
        tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError> {
        if tools.is_empty() {
            return Ok(LlmResponse::text("plan"));
        }

        let calls = self.calls;
        self.calls += 1;

        match calls {
            0 => Ok(LlmResponse::tool_call(ToolCall {
                name: "execute_command".to_string(),
                arguments: r#"{"command":"mkdir test"}"#.to_string(),
            })),
            1 => Ok(LlmResponse::tool_call(ToolCall {
                name: "ping".to_string(),
                arguments: "{}".to_string(),
            })),
            _ => Ok(LlmResponse::text("done")),
        }
    }
}
