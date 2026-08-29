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

/// The subset of `DEFAULT_ALLOWLIST` that changes what's on screen, as
/// opposed to only reading it. Drives two things: `ToolOutcome::mutated`
/// (via `is_mutating`, from the dispatcher) and the auto-verification
/// screenshot `call` attaches below.
const MUTATING_TOOLS: &[&str] = &["left_click", "double_click", "type", "key", "scroll"];

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

    pub fn is_mutating(name: &str) -> bool {
        MUTATING_TOOLS.contains(&name)
    }

    pub fn call(&mut self, name: &str, arguments: &str) -> Result<McpToolResult, McpError> {
        let arguments: serde_json::Value = serde_json::from_str(arguments)
            .map_err(|error| McpError::Protocol(error.to_string()))?;

        let mut result = self.client.call_tool(name, arguments)?;

        // A mutating action (click, type, key, scroll) gets a screenshot
        // attached to its own result automatically, so the model sees the
        // effect of what it just did without having to remember to ask —
        // and so there's real evidence to check even if it doesn't look
        // closely. Best-effort: if the verification screenshot itself
        // fails, or "screenshot" isn't in this toolset's allowlist, the
        // original action's result still stands.
        if Self::is_mutating(name)
            && self.handles("screenshot")
            && let Ok(verification) = self.client.call_tool("screenshot", serde_json::json!({}))
        {
            result.images.extend(verification.images);
            result.text = format!(
                "{}\n\n[Auto-verification screenshot attached — check it before answering.]",
                result.text
            );
        }

        Ok(result)
    }

    /// Exposed for tests, to inspect what was sent to the MCP client.
    pub fn client(&self) -> &M {
        &self.client
    }
}
