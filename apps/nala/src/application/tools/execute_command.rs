use crate::application::tools::{Tool, ToolDefinition};
use crate::ports::computer::{Computer, ComputerError};
use crate::ports::process::Process;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
pub struct ExecuteCommandArgs {
    /// Execute a command directly on the user's computer. Generate the
    /// command required to perform the user's requested action. The command
    /// is executed by the operating system shell. Use the appropriate
    /// command and syntax for the available operating system.
    pub command: String,
}

pub struct ExecuteCommandTool<C: Computer> {
    pub computer: C,
}

impl<C: Computer> ExecuteCommandTool<C> {
    pub fn new(computer: C) -> Self {
        Self { computer }
    }
}

impl<C: Computer> Tool for ExecuteCommandTool<C> {
    type Args = ExecuteCommandArgs;
    type Output = String;
    type Error = ComputerError;

    const NAME: &'static str = "execute_command";
    const DESCRIPTION: &'static str = "Execute a command directly on the user's computer. Generate the command required to perform the user's requested action. The command is executed by the operating system shell. Use the appropriate command and syntax for the available operating system.";

    fn parameters() -> serde_json::Value {
        serde_json::to_value(schemars::schema_for!(ExecuteCommandArgs))
            .expect("ExecuteCommandArgs schema should serialize to JSON")
    }

    fn execute(&mut self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let output = self.computer.execute_command(&args.command)?;

        if output.trim().is_empty() {
            Ok("SUCCESS: command completed with no output".to_string())
        } else {
            Ok(format!("SUCCESS: {output}"))
        }
    }

    fn parse_arguments(args: &str) -> Result<Self::Args, Self::Error> {
        serde_json::from_str(args).map_err(|error| ComputerError::CommandFailed(error.to_string()))
    }

    fn definition() -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: format!(
                "{} {}",
                C::SYSTEM_DESCRIPTION,
                <C::Process as Process>::SYSTEM_DESCRIPTION,
            ),
            parameters: Self::parameters(),
        }
    }

    fn context(&mut self) -> Result<String, Self::Error> {
        Ok(self.computer.get_context()?.to_string())
    }
}
