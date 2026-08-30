use crate::ports::llm::ToolCall;

/// What running a tool produced: text for the model to read, plus any
/// images (base64) it should see, e.g. from a vision-capable MCP tool.
/// Most tools never populate `images`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolOutcome {
    pub text: String,
    pub images: Vec<String>,
    /// Whether this call changed some external state (as opposed to only
    /// reading it). Set by the dispatcher, from the `Tool::MUTATING` of
    /// whichever native tool ran (MCP tools always report `false` — the
    /// protocol has no way to say). The agent loop uses this to require
    /// some evidence the effect was checked before the turn is allowed to
    /// end — see `agent_loop.rs`'s verification gate.
    pub mutated: bool,
}

impl From<String> for ToolOutcome {
    fn from(text: String) -> Self {
        Self {
            text,
            images: Vec::new(),
            mutated: false,
        }
    }
}

pub trait ToolDispatcher {
    type Output;
    type Error;

    fn dispatch(&mut self, tool_call: ToolCall) -> Result<Self::Output, Self::Error>;

    fn get_context(&mut self) -> Result<String, Self::Error>;
}
