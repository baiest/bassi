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
    /// Every command passed to `execute_command`, in order -- unlike
    /// `executed_command` (only the last one), this lets a test assert a
    /// capability tried more than one command (e.g. a fallback after the
    /// first one failed).
    pub commands_run: Vec<String>,
    pub should_fail: bool,
    /// When set, a command is only rejected if it contains this substring
    /// -- everything else still succeeds. Lets a test simulate one
    /// specific launch method failing (e.g. `start`) while another (e.g.
    /// `rundll32`) still works, without failing every command outright.
    pub fail_when_command_contains: Option<String>,
    pub should_fail_context: bool,
    pub output: String,
}

impl FakeComputer {
    pub fn new() -> Self {
        Self {
            executed_command: None,
            commands_run: Vec::new(),
            should_fail: false,
            fail_when_command_contains: None,
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
        if let Some(needle) = &self.fail_when_command_contains
            && name.contains(needle.as_str())
        {
            return Err(ComputerError::CommandFailed(format!(
                "fake computer failed: {name}"
            )));
        }

        self.executed_command = Some(name.to_string());
        self.commands_run.push(name.to_string());

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
