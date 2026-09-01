use std::time::Duration;

use device_capabilities::ports::computer::{Computer, ComputerContext, ComputerError};
use device_capabilities::ports::process::{Process, ProcessError};

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
}

impl FakeComputer {
    pub fn new() -> Self {
        Self {
            executed_command: None,
        }
    }
}

impl Computer for FakeComputer {
    type Process = FakeProcess;

    const SYSTEM_DESCRIPTION: &'static str = "This is a fake computer.";

    fn execute_command(&mut self, name: &str, _timeout: Duration) -> Result<String, ComputerError> {
        self.executed_command = Some(name.to_string());
        Ok(String::new())
    }

    fn get_context(&mut self) -> Result<ComputerContext, ComputerError> {
        Ok(ComputerContext {
            system: Self::SYSTEM_DESCRIPTION.to_string(),
            username: "fake_user".to_string(),
            home_dir: "C:\\fake_home".to_string(),
            desktop_dir: "C:\\fake_home\\Desktop".to_string(),
            current_dir: "C:\\fake_home".to_string(),
        })
    }
}
