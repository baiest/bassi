use std::process::Command;

use crate::ports::process::{Process, ProcessError};

pub struct Windows;

impl Windows {
    pub fn new() -> Self {
        Self
    }
}

impl Default for Windows {
    fn default() -> Self {
        Self::new()
    }
}

impl Process for Windows {
    const SYSTEM_DESCRIPTION: &'static str =
        "Commands are executed using Windows cmd.exe EXCLUSIVE. IMPORTANT Use Windows cmd syntax.";

    fn spawn(&mut self, command: &str, args: &[&str]) -> Result<String, ProcessError> {
        if command.trim().is_empty() {
            return Err(ProcessError::InvalidArguments(
                "Command cannot be empty".to_string(),
            ));
        }

        let output = Command::new(command)
            .args(args)
            .output()
            .map_err(|error| ProcessError::ProcessFailed(error.to_string()))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);

            return Err(ProcessError::ProcessFailed(format!(
                "Command failed: {command} {:?}\n{}",
                args, error
            )));
        }

        String::from_utf8(output.stdout)
            .map_err(|error| ProcessError::ProcessFailed(error.to_string()))
    }
}
