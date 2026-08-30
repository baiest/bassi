use mcp::{McpClient, McpError, McpToolInfo, McpToolResult};

/// A scripted `McpClient` for testing `McpToolset` without a real MCP
/// server. `tools` is what `list_tools` returns; `call_result` (or
/// `call_error`) drives `call_tool`, and every call is recorded so tests can
/// assert what name/arguments were actually sent.
#[derive(Default)]
pub struct FakeMcpClient {
    pub tools: Vec<McpToolInfo>,
    pub call_result: Option<McpToolResult>,
    pub call_error: Option<String>,
    pub calls: Vec<(String, serde_json::Value)>,
}

impl FakeMcpClient {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_tool(mut self, name: &str, description: &str) -> Self {
        self.tools.push(McpToolInfo {
            name: name.to_string(),
            description: description.to_string(),
            parameters: serde_json::json!({"type": "object"}),
        });
        self
    }

    pub fn returning(mut self, result: McpToolResult) -> Self {
        self.call_result = Some(result);
        self
    }

    pub fn failing_calls_with(mut self, message: &str) -> Self {
        self.call_error = Some(message.to_string());
        self
    }
}

impl McpClient for FakeMcpClient {
    fn list_tools(&mut self) -> Result<Vec<McpToolInfo>, McpError> {
        Ok(self.tools.clone())
    }

    fn call_tool(
        &mut self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<McpToolResult, McpError> {
        self.calls.push((name.to_string(), arguments));

        if let Some(message) = &self.call_error {
            return Err(McpError::ToolFailed(message.clone()));
        }

        Ok(self.call_result.clone().unwrap_or_default())
    }
}
