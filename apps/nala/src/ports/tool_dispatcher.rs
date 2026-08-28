use crate::ports::llm::ToolCall;

/// What running a tool produced: text for the model to read, plus any
/// images (base64) it should see — e.g. a screenshot from a computer-use
/// tool. Most tools never populate `images`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolOutcome {
    pub text: String,
    pub images: Vec<String>,
}

impl From<String> for ToolOutcome {
    fn from(text: String) -> Self {
        Self {
            text,
            images: Vec::new(),
        }
    }
}

pub trait ToolDispatcher {
    type Output;
    type Error;

    fn dispatch(&mut self, tool_call: ToolCall) -> Result<Self::Output, Self::Error>;

    fn get_context(&mut self) -> Result<String, Self::Error>;
}
