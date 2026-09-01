use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("process failed: {0}")]
    ProcessFailed(String),
    #[error("invalid arguments: {0}")]
    InvalidArguments(String),
    #[error("process timed out after {0:?} and was killed")]
    Timeout(Duration),
}

pub trait Process {
    const SYSTEM_DESCRIPTION: &'static str;

    fn spawn(
        &mut self,
        program: &str,
        args: &[&str],
        timeout: Duration,
    ) -> Result<String, ProcessError>;
}
