use std::time::Duration;

use crate::application::tools::{Tool, ToolDefinition};
use crate::ports::computer::{Computer, ComputerError};
use crate::ports::process::Process;
use schemars::JsonSchema;
use serde::Deserialize;

/// How long a shell command may run before it's killed. Generous, because
/// legitimate commands (`start chrome`, installers, ...) can take a while
/// to hand back control, but bounded so a command that never returns
/// doesn't hang the turn forever.
pub const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

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
    pub timeout: Duration,
}

impl<C: Computer> ExecuteCommandTool<C> {
    pub fn new(computer: C) -> Self {
        Self {
            computer,
            timeout: DEFAULT_COMMAND_TIMEOUT,
        }
    }

    pub fn with_timeout(computer: C, timeout: Duration) -> Self {
        Self { computer, timeout }
    }
}

impl<C: Computer> Tool for ExecuteCommandTool<C> {
    type Args = ExecuteCommandArgs;
    type Output = String;
    type Error = ComputerError;

    const NAME: &'static str = "execute_command";
    const DESCRIPTION: &'static str = "Execute a command directly on the user's computer. Generate the command required to perform the user's requested action. The command is executed by the operating system shell. Use the appropriate command and syntax for the available operating system.";
    const MUTATING: bool = true;

    fn parameters() -> serde_json::Value {
        serde_json::to_value(schemars::schema_for!(ExecuteCommandArgs))
            .expect("ExecuteCommandArgs schema should serialize to JSON")
    }

    fn execute(&mut self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let output = self.computer.execute_command(&args.command, self.timeout)?;

        if output.trim().is_empty() {
            Ok("Command executed, no output. This confirms the command ran; it does not confirm the requested outcome happened on screen. Verify with a screenshot before answering.".to_string())
        } else {
            Ok(format!(
                "Command executed. Output: {output}\n\nThis confirms the command ran; it does not confirm the requested outcome happened on screen. Verify with a screenshot before answering."
            ))
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
