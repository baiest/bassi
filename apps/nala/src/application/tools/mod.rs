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
    const PARAMETERS: &'static str;

    fn execute(&mut self, args: Self::Args) -> Result<Self::Output, Self::Error>;

    fn parse_arguments(arguments: &str) -> Result<Self::Args, Self::Error>;

    fn definition() -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME,
            description: Self::DESCRIPTION.to_string(),
            parameters: Self::PARAMETERS,
        }
    }

    fn context(&mut self) -> Result<String, Self::Error>;
}
