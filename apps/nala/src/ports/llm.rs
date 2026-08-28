use serde::Serialize;

use crate::application::tools::ToolDefinition;

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
    #[error("invalid tool definition: {0}")]
    InvalidToolDefinition(String),
}

#[derive(Debug)]
pub enum LlmResponse {
    Text(String),
    ToolCall(ToolCall),
}

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub tool_calls: Option<Vec<ToolCall>>,
}

pub trait Llm {
    fn generate(
        &mut self,
        messages: &[Message],
        tools: &[&ToolDefinition],
    ) -> Result<LlmResponse, LlmError>;
}
