pub mod computer_use;
pub mod dispatcher;
pub mod execute_command;
pub mod ping;
pub mod registry;

// Re-exported so existing `application::tools::ToolDefinition` call sites
// keep working — the type itself lives in ports/tool.rs since it crosses
// the application/adapter boundary (Llm and ToolDispatcher ports use it).
pub use crate::ports::tool::ToolDefinition;

pub trait Tool {
    type Args;
    type Output;
    type Error;

    const NAME: &'static str;
    const DESCRIPTION: &'static str;
    /// Whether calling this tool changes something on the user's computer.
    /// Read-only tools (a screenshot, listing windows) leave this `false`;
    /// tools that act (running a command, clicking, typing) set it `true`
    /// so the dispatcher knows to attach evidence of the effect. See
    /// `ToolOutcome::mutated`.
    const MUTATING: bool = false;

    /// The JSON schema for `Args`, published to the LLM alongside the tool's
    /// name and description. Implementations should derive this from `Args`
    /// itself (e.g. via `schemars::schema_for!`) rather than writing it by
    /// hand, so the two can never drift apart.
    fn parameters() -> serde_json::Value;

    fn execute(&mut self, args: Self::Args) -> Result<Self::Output, Self::Error>;

    fn parse_arguments(arguments: &str) -> Result<Self::Args, Self::Error>;

    fn definition() -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: Self::DESCRIPTION.to_string(),
            parameters: Self::parameters(),
        }
    }

    fn context(&mut self) -> Result<String, Self::Error>;
}
