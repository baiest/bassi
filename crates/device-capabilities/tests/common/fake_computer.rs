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
    /// Every command passed to `execute_command_detached`, in order -- lets
    /// a test assert a capability used the detached (no-output) path
    /// instead of `execute_command`. `execute_command_detached` also still
    /// records into `executed_command`/`commands_run` above, so an existing
    /// assertion on those keeps working regardless of which path a
    /// capability calls.
    pub detached_commands: Vec<String>,
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
            detached_commands: Vec::new(),
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

    fn execute_command_detached(
        &mut self,
        name: &str,
        timeout: Duration,
    ) -> Result<(), ComputerError> {
        // Recorded only on success, same as `commands_run` above -- a
        // command that failed (e.g. `fail_when_command_contains`) was never
        // actually "run" as far as either log is concerned.
        self.execute_command(name, timeout).inspect(|_| {
            self.detached_commands.push(name.to_string());
        })?;
        Ok(())
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
