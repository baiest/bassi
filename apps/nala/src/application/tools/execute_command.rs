use crate::application::tools::Tool;
use crate::ports::computer::{Computer, ComputerError};

pub struct ExecuteCommandArgs {
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
    type Output = ();
    type Error = ComputerError;

    const NAME: &'static str = "execute_command";
    const DESCRIPTION: &'static str = "Execute a command on the computer";

    fn execute(&mut self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.computer.execute_command(&args.command)
    }
}
