#[derive(Debug, Clone)]
pub struct McpToolInfo {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct McpToolResult {
    pub text: String,
    /// Base64-encoded image data returned by the tool call (e.g. a
    /// screenshot), in the order the server returned them.
    pub images: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("transport error: {0}")]
    Transport(String),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("tool call failed: {0}")]
    ToolFailed(String),
}

pub trait McpClient {
    fn list_tools(&mut self) -> Result<Vec<McpToolInfo>, McpError>;

    fn call_tool(
        &mut self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<McpToolResult, McpError>;
}
