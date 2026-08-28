use crate::ports::llm::ToolCall;

pub trait ToolDispatcher {
    type Output;
    type Error;

    fn dispatch(&mut self, tool_call: ToolCall) -> Result<Self::Output, Self::Error>;

    fn get_context(&mut self) -> Result<String, Self::Error>;
}
