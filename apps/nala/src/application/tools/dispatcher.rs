use crate::application::tools::Tool;

#[derive(Debug)]
pub enum ToolDispatcherError<E> {
    ToolNotFound,
    Tool(E),
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

        tool.execute(args).map_err(ToolDispatcherError::Tool)
    }
}

impl<T> Default for ToolDispatcher<T> {
    fn default() -> Self {
        Self::new()
    }
}
