use crate::application::tools::ToolDefinition;
use crate::ports::mcp::{McpClient, McpError, McpToolResult};

/// MCP servers like computer-use-mcp expose dozens of tools; publishing all
/// of them to a local model floods the prompt and hurts tool selection. This
/// is the default set exposed to the LLM — enough to see the screen and
/// drive the mouse/keyboard.
pub const DEFAULT_ALLOWLIST: &[&str] = &[
    "screenshot",
    "left_click",
    "double_click",
    "type",
    "key",
    "scroll",
    "get_display_size",
    "list_windows",
];

/// A dynamically-discovered set of MCP tools, filtered down to an allowlist.
/// Unlike `Tool` implementations (one const `NAME`, one typed `Args`), the
/// set of tools and their schemas are only known once connected to the MCP
/// server, so this owns a client and a list of definitions instead.
pub struct ComputerUseToolset<M: McpClient> {
    client: M,
    allowed: Vec<String>,
    definitions: Vec<ToolDefinition>,
}

impl<M: McpClient> ComputerUseToolset<M> {
    pub fn connect(mut client: M, allowed: &[&str]) -> Result<Self, McpError> {
        let allowed: Vec<String> = allowed.iter().map(|name| name.to_string()).collect();

        let definitions = client
            .list_tools()?
            .into_iter()
            .filter(|tool| allowed.contains(&tool.name))
            .map(|tool| ToolDefinition {
                name: tool.name,
                description: tool.description,
                parameters: tool.parameters,
            })
            .collect();

        Ok(Self {
            client,
            allowed,
            definitions,
        })
    }

    pub fn definitions(&self) -> &[ToolDefinition] {
        &self.definitions
    }

    pub fn handles(&self, name: &str) -> bool {
        self.allowed.iter().any(|allowed| allowed == name)
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
