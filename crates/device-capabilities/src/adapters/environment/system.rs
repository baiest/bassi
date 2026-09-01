use crate::ports::environment::{Environment, EnvironmentError};

#[derive(Default)]
pub struct SystemEnvironment;

impl SystemEnvironment {
    pub fn new() -> Self {
        Self
    }
}

impl Environment for SystemEnvironment {
    fn var(&self, key: &str) -> Result<String, EnvironmentError> {
        std::env::var(key)
            .map_err(|error| EnvironmentError::VarNotSet(key.to_string(), error.to_string()))
    }

    fn current_dir(&self) -> Result<String, EnvironmentError> {
        std::env::current_dir()
            .map(|path| path.to_string_lossy().to_string())
            .map_err(|error| EnvironmentError::CurrentDirUnavailable(error.to_string()))
    }
}
