use std::time::Duration;

use nala::ports::process::{Process, ProcessError};

pub struct FakeProcess {
    pub spawned: Option<String>,
    pub should_fail: bool,
    pub should_timeout: bool,
    pub last_timeout: Option<Duration>,
}

impl FakeProcess {
    pub fn new() -> Self {
        Self {
            spawned: None,
            should_fail: false,
            should_timeout: false,
            last_timeout: None,
        }
    }
}

impl Process for FakeProcess {
    const SYSTEM_DESCRIPTION: &'static str = "This is a fake process.";

    fn spawn(
        &mut self,
        program: &str,
        _args: &[&str],
        timeout: Duration,
    ) -> Result<String, ProcessError> {
        self.last_timeout = Some(timeout);

        if self.should_timeout {
            return Err(ProcessError::Timeout(timeout));
        }

        if self.should_fail {
            return Err(ProcessError::ProcessFailed(
                "fake process failed".to_string(),
            ));
        }

        self.spawned = Some(program.to_string());

        Ok(String::new())
    }
}
