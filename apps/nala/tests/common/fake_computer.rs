use std::time::Duration;

use nala::ports::computer::{Computer, ComputerContext, ComputerError};
use nala::ports::process::{Process, ProcessError};

pub struct FakeProcess;

impl Process for FakeProcess {
    const SYSTEM_DESCRIPTION: &'static str = "This is a fake process.";

    fn spawn(
        &mut self,
        _program: &str,
        _args: &[&str],
        _timeout: Duration,
    ) -> Result<String, ProcessError> {
        Ok(String::new())
    }
}

pub struct FakeComputer {
    pub executed_command: Option<String>,
    pub should_fail: bool,
    pub should_fail_context: bool,
    pub output: String,
}

impl FakeComputer {
    pub fn new() -> Self {
        Self {
            executed_command: None,
            should_fail: false,
            should_fail_context: false,
            output: String::new(),
        }
    }
}

impl Computer for FakeComputer {
    type Process = FakeProcess;

    const SYSTEM_DESCRIPTION: &'static str = "This is a fake computer.";

    fn execute_command(&mut self, name: &str, _timeout: Duration) -> Result<String, ComputerError> {
        if self.should_fail {
            return Err(ComputerError::CommandFailed(
                "fake computer failed".to_string(),
            ));
        }

        self.executed_command = Some(name.to_string());

        Ok(self.output.clone())
    }

    fn get_context(&mut self) -> Result<ComputerContext, ComputerError> {
        if self.should_fail_context {
            return Err(ComputerError::CommandFailed(
                "fake computer failed to get context".to_string(),
            ));
        }

        Ok(ComputerContext {
            system: Self::SYSTEM_DESCRIPTION.to_string(),
            username: "fake_user".to_string(),
            home_dir: "C:\\fake_home".to_string(),
            desktop_dir: "C:\\fake_home\\Desktop".to_string(),
            current_dir: "C:\\fake_home".to_string(),
        })
    }
}
