use std::collections::HashMap;

use nala::ports::environment::{Environment, EnvironmentError};

pub struct FakeEnvironment {
    pub vars: HashMap<String, String>,
    pub current_dir: String,
    pub should_fail: bool,
}

impl FakeEnvironment {
    pub fn new() -> Self {
        Self {
            vars: HashMap::new(),
            current_dir: String::new(),
            should_fail: false,
        }
    }

    pub fn with_var(mut self, key: &str, value: &str) -> Self {
        self.vars.insert(key.to_string(), value.to_string());
        self
    }

    pub fn with_current_dir(mut self, value: &str) -> Self {
        self.current_dir = value.to_string();
        self
    }
}

impl Default for FakeEnvironment {
    fn default() -> Self {
        Self::new()
    }
}

impl Environment for FakeEnvironment {
    fn var(&self, key: &str) -> Result<String, EnvironmentError> {
        if self.should_fail {
            return Err(EnvironmentError::VarNotSet(
                key.to_string(),
                "fake environment failed".to_string(),
            ));
        }

        self.vars
            .get(key)
            .cloned()
            .ok_or_else(|| EnvironmentError::VarNotSet(key.to_string(), "not set".to_string()))
    }

    fn current_dir(&self) -> Result<String, EnvironmentError> {
        if self.should_fail {
            return Err(EnvironmentError::CurrentDirUnavailable(
                "fake environment failed".to_string(),
            ));
        }

        Ok(self.current_dir.clone())
    }
}
