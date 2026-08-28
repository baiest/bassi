use crate::{
    application::tools::{Tool, execute_command::ExecuteCommandTool, ping::PingTool},
    ports::{
        computer::Computer, llm::ToolCall, tool_dispatcher::ToolDispatcher as ToolDispatcherPort,
    },
};

#[derive(Debug)]
pub enum ToolDispatcherError {
    ToolNotFound,
    ToolErrorParsingArguments(String),
    ToolExecuteError(String),
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
                            ToolDispatcherError::ToolErrorParsingArguments(format!("{error:?}"))
                        })?;

                    return tool.execute(args).map_err(|error| {
                        ToolDispatcherError::ToolExecuteError(format!("{error:?}"))
                    });
                }
                Tools::Ping(tool) if tool_call.name == PingTool::NAME => {
                    PingTool::parse_arguments(&tool_call.arguments).map_err(|error| {
                        ToolDispatcherError::ToolErrorParsingArguments(format!("{error:?}"))
                    })?;

                    return tool.execute(()).map_err(|error| {
                        ToolDispatcherError::ToolExecuteError(format!("{error:?}"))
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
                    .map_err(|error| ToolDispatcherError::ToolExecuteError(format!("{error:?}")));
            }
        }

        Err(ToolDispatcherError::ToolNotFound)
    }
}
