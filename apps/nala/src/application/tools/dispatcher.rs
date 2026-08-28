use crate::{
    application::tools::{
        Tool, computer_use::ComputerUseToolset, execute_command::ExecuteCommandTool, ping::PingTool,
    },
    ports::{
        computer::Computer,
        llm::ToolCall,
        mcp::McpClient,
        tool_dispatcher::{ToolDispatcher as ToolDispatcherPort, ToolOutcome},
    },
};

type BoxedError = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Debug, thiserror::Error)]
pub enum ToolDispatcherError {
    #[error("tool not found")]
    ToolNotFound,
    #[error("failed to parse tool call arguments: {0}")]
    ToolErrorParsingArguments(#[source] BoxedError),
    #[error("tool execution failed: {0}")]
    ToolExecuteError(#[source] BoxedError),
}

/// A `McpClient` that is never actually connected to anything — the default
/// for `M` so callers that don't use `Tools::ComputerUse` (most tests, and
/// any setup without computer-use-mcp) don't have to name an MCP client
/// type at all.
#[derive(Debug, Default)]
pub struct NoMcpClient;

impl McpClient for NoMcpClient {
    fn list_tools(
        &mut self,
    ) -> Result<Vec<crate::ports::mcp::McpToolInfo>, crate::ports::mcp::McpError> {
        Ok(Vec::new())
    }

    fn call_tool(
        &mut self,
        _name: &str,
        _arguments: serde_json::Value,
    ) -> Result<crate::ports::mcp::McpToolResult, crate::ports::mcp::McpError> {
        Err(crate::ports::mcp::McpError::Transport(
            "NoMcpClient is never connected".to_string(),
        ))
    }
}

/// One variant per `Tool` implementation the dispatcher knows how to run.
/// Adding a tool means adding a variant here and a match arm below — both
/// checked exhaustively at compile time, no runtime type erasure.
pub enum Tools<C: Computer, M: McpClient = NoMcpClient> {
    ExecuteCommand(ExecuteCommandTool<C>),
    Ping(PingTool),
    ComputerUse(ComputerUseToolset<M>),
}

pub struct ToolDispatcher<C: Computer, M: McpClient = NoMcpClient> {
    tools: Vec<Tools<C, M>>,
}

impl<C: Computer, M: McpClient> ToolDispatcher<C, M> {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    pub fn register(&mut self, tool: Tools<C, M>) {
        self.tools.push(tool);
    }
}

impl<C: Computer, M: McpClient> Default for ToolDispatcher<C, M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: Computer, M: McpClient> ToolDispatcherPort for ToolDispatcher<C, M> {
    type Output = ToolOutcome;
    type Error = ToolDispatcherError;

    fn dispatch(&mut self, tool_call: ToolCall) -> Result<Self::Output, Self::Error> {
        for tool in &mut self.tools {
            match tool {
                Tools::ExecuteCommand(tool) if tool_call.name == ExecuteCommandTool::<C>::NAME => {
                    let args = ExecuteCommandTool::<C>::parse_arguments(&tool_call.arguments)
                        .map_err(|error| {
                            ToolDispatcherError::ToolErrorParsingArguments(Box::new(error))
                        })?;

                    return tool
                        .execute(args)
                        .map(ToolOutcome::from)
                        .map_err(|error| ToolDispatcherError::ToolExecuteError(Box::new(error)));
                }
                Tools::Ping(tool) if tool_call.name == PingTool::NAME => {
                    PingTool::parse_arguments(&tool_call.arguments).map_err(|error| {
                        ToolDispatcherError::ToolErrorParsingArguments(Box::new(error))
                    })?;

                    return tool
                        .execute(())
                        .map(ToolOutcome::from)
                        .map_err(|error| ToolDispatcherError::ToolExecuteError(Box::new(error)));
                }
                Tools::ComputerUse(toolset) if toolset.handles(&tool_call.name) => {
                    let result = toolset
                        .call(&tool_call.name, &tool_call.arguments)
                        .map_err(|error| ToolDispatcherError::ToolExecuteError(Box::new(error)))?;

                    return Ok(ToolOutcome {
                        text: result.text,
                        images: result.images,
                    });
                }
                _ => continue,
            }
        }

        Err(ToolDispatcherError::ToolNotFound)
    }

    fn get_context(&mut self) -> Result<String, Self::Error> {
        for tool in &mut self.tools {
            if let Tools::ExecuteCommand(tool) = tool {
                return tool
                    .context()
                    .map_err(|error| ToolDispatcherError::ToolExecuteError(Box::new(error)));
            }
        }

        Err(ToolDispatcherError::ToolNotFound)
    }
}
