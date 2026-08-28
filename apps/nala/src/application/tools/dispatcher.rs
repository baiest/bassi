use crate::{
    application::tools::{Tool, execute_command::ExecuteCommandTool, ping::PingTool},
    ports::{
        computer::Computer, llm::ToolCall, tool_dispatcher::ToolDispatcher as ToolDispatcherPort,
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

/// One variant per `Tool` implementation the dispatcher knows how to run.
/// Adding a tool means adding a variant here and a match arm below — both
/// checked exhaustively at compile time, no runtime type erasure.
pub enum Tools<C: Computer> {
    ExecuteCommand(ExecuteCommandTool<C>),
    Ping(PingTool),
}

pub struct ToolDispatcher<C: Computer> {
    tools: Vec<Tools<C>>,
}

impl<C: Computer> ToolDispatcher<C> {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    pub fn register(&mut self, tool: Tools<C>) {
        self.tools.push(tool);
    }
}

impl<C: Computer> Default for ToolDispatcher<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: Computer> ToolDispatcherPort for ToolDispatcher<C> {
    type Output = String;
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
                        .map_err(|error| ToolDispatcherError::ToolExecuteError(Box::new(error)));
                }
                Tools::Ping(tool) if tool_call.name == PingTool::NAME => {
                    PingTool::parse_arguments(&tool_call.arguments).map_err(|error| {
                        ToolDispatcherError::ToolErrorParsingArguments(Box::new(error))
                    })?;

                    return tool
                        .execute(())
                        .map_err(|error| ToolDispatcherError::ToolExecuteError(Box::new(error)));
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
