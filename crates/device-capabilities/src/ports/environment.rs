#[derive(Debug, thiserror::Error)]
pub enum EnvironmentError {
    #[error("environment variable {0} is not set: {1}")]
    VarNotSet(String, String),
    #[error("current directory unavailable: {0}")]
    CurrentDirUnavailable(String),
}

/// Reads process-level environment state (variables, current directory).
/// Kept as its own port so adapters that need it (like the Windows
/// `Computer`) don't have to call `std::env` directly, which can't be
/// substituted in tests.
pub trait Environment {
    fn var(&self, key: &str) -> Result<String, EnvironmentError>;
    fn current_dir(&self) -> Result<String, EnvironmentError>;
}
