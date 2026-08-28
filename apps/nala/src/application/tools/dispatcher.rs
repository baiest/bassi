use crate::{
    application::tools::Tool,
    ports::{llm::ToolCall, tool_dispatcher::ToolDispatcher as ToolDispatcherPort},
};

#[derive(Debug)]
pub enum ToolDispatcherError<E> {
    ToolNotFound,
    ToolErrorParsingArguments(E),
    ToolExecuteError(E),
}

pub struct ToolDispatcher<T> {
    tool: Option<T>,
}

impl<T> ToolDispatcher<T> {
    pub fn new() -> Self {
        Self { tool: None }
    }

    pub fn register(&mut self, tool: T) {
        self.tool = Some(tool);
    }
}

impl<T> ToolDispatcher<T>
where
    T: Tool,
{
    pub fn execute(
        &mut self,
        name: &str,
        args: T::Args,
    ) -> Result<T::Output, ToolDispatcherError<T::Error>> {
        if name != T::NAME {
            return Err(ToolDispatcherError::ToolNotFound);
        }

        let tool = self
            .tool
            .as_mut()
            .ok_or(ToolDispatcherError::ToolNotFound)?;

        tool.execute(args)
            .map_err(ToolDispatcherError::ToolExecuteError)
    }
}

impl<T> ToolDispatcherPort for ToolDispatcher<T>
where
    T: Tool,
{
    type Output = T::Output;
    type Error = ToolDispatcherError<T::Error>;

    fn dispatch(&mut self, tool_call: ToolCall) -> Result<Self::Output, Self::Error> {
        if tool_call.name != T::NAME {
            return Err(ToolDispatcherError::ToolNotFound);
        }

        let tool = self
            .tool
            .as_mut()
            .ok_or(ToolDispatcherError::ToolNotFound)?;

        let args = T::parse_arguments(&tool_call.arguments)
            .map_err(ToolDispatcherError::ToolErrorParsingArguments)?;

        tool.execute(args)
            .map_err(ToolDispatcherError::ToolExecuteError)
    }

    fn get_context(&mut self) -> Result<String, Self::Error> {
        let tool = self
            .tool
            .as_mut()
            .ok_or(ToolDispatcherError::ToolNotFound)?;

        tool.context()
            .map_err(ToolDispatcherError::ToolExecuteError)
    }
}

impl<T> Default for ToolDispatcher<T> {
    fn default() -> Self {
        Self::new()
    }
}
