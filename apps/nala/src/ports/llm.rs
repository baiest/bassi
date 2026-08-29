use serde::Serialize;

use crate::ports::tool::ToolDefinition;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("request failed: {0}")]
    RequestFailed(String),
    #[error("invalid response: {0}")]
    InvalidResponse(String),
}

/// Token accounting for one completed LLM call, when the backend reports
/// it (Ollama does, via `prompt_eval_count`/`eval_count`). `None` fields
/// mean the backend didn't report that count, not that it was zero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
}

/// A single LLM turn. `tool_calls` may hold more than one entry — a model
/// can request several actions in one response (e.g. "screenshot" won't do
/// that, but many models batch independent calls) — and the loop executes
/// all of them before calling the model again. `text` and `tool_calls` are
/// not mutually exclusive: a model may emit both a short remark and one or
/// more tool calls in the same turn.
#[derive(Debug, Clone, Default)]
pub struct LlmResponse {
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Usage,
}

impl LlmResponse {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            tool_calls: Vec::new(),
            usage: Usage::default(),
        }
    }

    pub fn tool_call(tool_call: ToolCall) -> Self {
        Self {
            text: None,
            tool_calls: vec![tool_call],
            usage: Usage::default(),
        }
    }

    pub fn tool_calls(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            text: None,
            tool_calls,
            usage: Usage::default(),
        }
    }

    /// A response is answerable (can end the turn) only if it carries no
    /// pending tool calls and non-empty text.
    pub fn is_final_answer(&self) -> bool {
        self.tool_calls.is_empty()
            && self
                .text
                .as_deref()
                .is_some_and(|text| !text.trim().is_empty())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Set only on `role: "tool"` messages, so the LLM can correlate a
    /// result with the tool call that produced it.
    pub tool_name: Option<String>,
    /// Base64-encoded images attached to this message (e.g. a screenshot
    /// returned by a computer-use tool), so a vision-capable model can see
    /// them. Empty for ordinary text-only messages.
    pub images: Vec<String>,
}

pub trait Llm {
    fn generate(
        &mut self,
        messages: &[Message],
        tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError>;
}
