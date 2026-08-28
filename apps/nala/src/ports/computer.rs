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
}

pub trait Computer {
    type Process: Process;

    const SYSTEM_DESCRIPTION: &'static str;

    fn execute_command(&mut self, command: &str) -> Result<String, ComputerError>;
    fn get_context(&mut self) -> Result<ComputerContext, ComputerError>;
}
