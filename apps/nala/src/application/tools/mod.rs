pub mod dispatcher;
pub mod execute_command;
pub mod registry;

pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
}

pub trait Tool {
    type Args;
    type Output;
    type Error;

    const NAME: &'static str;
    const DESCRIPTION: &'static str;

    fn execute(&mut self, args: Self::Args) -> Result<Self::Output, Self::Error>;
    fn definition() -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME,
            description: Self::DESCRIPTION,
        }
    }
}
