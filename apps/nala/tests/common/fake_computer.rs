use nala::ports::computer::{Computer, ComputerError};

pub struct FakeComputer {
    pub executed_command: Option<String>,
    pub should_fail: bool,
}

impl FakeComputer {
    pub fn new() -> Self {
        Self {
            executed_command: None,
            should_fail: false,
        }
    }
}

impl Computer for FakeComputer {
    fn execute_command(&mut self, name: &str) -> Result<(), ComputerError> {
        if self.should_fail {
            return Err(ComputerError::CommandFailed);
        }

        self.executed_command = Some(name.to_string());

        Ok(())
    }
}
