use std::time::Duration;

use crate::ports::environment::EnvironmentError;
use crate::ports::process::Process;

#[derive(Debug)]
pub struct ComputerContext {
    pub system: String,
    pub username: String,
    pub home_dir: String,
    pub desktop_dir: String,
    pub current_dir: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ComputerError {
    #[error("command failed: {0}")]
    CommandFailed(String),
    #[error("environment error: {0}")]
    Environment(#[from] EnvironmentError),
}

pub trait Computer {
    type Process: Process;

    const SYSTEM_DESCRIPTION: &'static str;

    fn execute_command(
        &mut self,
        command: &str,
        timeout: Duration,
    ) -> Result<String, ComputerError>;

    /// Mirrors `Process::spawn_detached`: runs `command` without capturing
    /// its output, so a fire-and-forget launcher (`open_app`, `open_url`)
    /// never opens a pipe a GUI grandchild could inherit and hold open
    /// forever -- see BAS-61. Default body delegates to `execute_command`,
    /// so every existing `Computer` implementation keeps compiling
    /// unchanged.
    fn execute_command_detached(
        &mut self,
        command: &str,
        timeout: Duration,
    ) -> Result<(), ComputerError> {
        self.execute_command(command, timeout).map(|_| ())
    }

    fn get_context(&mut self) -> Result<ComputerContext, ComputerError>;
}
