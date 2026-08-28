#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("process failed: {0}")]
    ProcessFailed(String),
    #[error("invalid arguments: {0}")]
    InvalidArguments(String),
}

pub trait Process {
    const SYSTEM_DESCRIPTION: &'static str;

    fn spawn(&mut self, program: &str, args: &[&str]) -> Result<String, ProcessError>;
}
