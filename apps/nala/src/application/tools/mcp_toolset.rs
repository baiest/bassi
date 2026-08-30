use mcp::{McpClient, McpError, McpToolResult};

use crate::application::tools::ToolDefinition;

/// A dynamically-discovered set of MCP tools, optionally filtered down to an
/// allowlist. Unlike `Tool` implementations (one const `NAME`, one typed
/// `Args`), the set of tools and their schemas are only known once
/// connected to the MCP server, so this owns a client and a list of
/// definitions instead.
pub struct McpToolset<M: McpClient> {
    client: M,
    definitions: Vec<ToolDefinition>,
}

impl<M: McpClient> McpToolset<M> {
    /// `allowed` narrows the tools published to the LLM to a caller-chosen
    /// subset — useful when a server exposes dozens of tools and publishing
    /// all of them would flood a local model's prompt and hurt tool
    /// selection. `None` publishes every tool the server reports.
    pub fn connect(mut client: M, allowed: Option<&[&str]>) -> Result<Self, McpError> {
        let tools = client.list_tools()?;

        let definitions = tools
            .into_iter()
            .filter(|tool| allowed.is_none_or(|allowed| allowed.contains(&tool.name.as_str())))
            .map(|tool| ToolDefinition {
                name: tool.name,
                description: tool.description,
                parameters: tool.parameters,
            })
            .collect();

        Ok(Self {
            client,
            definitions,
        })
    }

    pub fn definitions(&self) -> &[ToolDefinition] {
        &self.definitions
    }

    pub fn handles(&self, name: &str) -> bool {
        self.definitions.iter().any(|tool| tool.name == name)
    }

    pub fn call(&mut self, name: &str, arguments: &str) -> Result<McpToolResult, McpError> {
        let arguments: serde_json::Value = serde_json::from_str(arguments)
            .map_err(|error| McpError::Protocol(error.to_string()))?;

        self.client.call_tool(name, arguments)
    }

    /// Exposed for tests, to inspect what was sent to the MCP client.
    pub fn client(&self) -> &M {
        &self.client
    }
}
