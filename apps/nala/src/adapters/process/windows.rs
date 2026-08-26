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
    fn spawn(&mut self, program: &str, args: &[&str]) -> Result<(), ProcessError> {
        Command::new(program)
            .args(args)
            .spawn()
            .map(|_| ())
            .map_err(|_| ProcessError::ProcessFailed)
    }
}
