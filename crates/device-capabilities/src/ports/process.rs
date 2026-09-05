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

    /// Launches without capturing output -- same success/failure contract as
    /// `spawn`, but nothing is read back. "Detached" means output-detached,
    /// not process-detached: the child is still tracked and killed on
    /// timeout the same way. Exists so a fire-and-forget launcher (`start
    /// "" "<app>"`) never has to open a pipe a GUI grandchild might inherit
    /// and hold open forever -- see BAS-61. Default body delegates to
    /// `spawn`, so every existing `Process` implementation keeps compiling
    /// unchanged; only the real adapter needs to override it.
    fn spawn_detached(
        &mut self,
        program: &str,
        args: &[&str],
        timeout: Duration,
    ) -> Result<(), ProcessError> {
        self.spawn(program, args, timeout).map(|_| ())
    }
}
