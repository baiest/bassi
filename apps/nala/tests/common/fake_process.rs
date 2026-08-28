use nala::ports::process::{Process, ProcessError};

pub struct FakeProcess {
    pub spawned: Option<String>,
    pub should_fail: bool,
}

impl FakeProcess {
    pub fn new() -> Self {
        Self {
            spawned: None,
            should_fail: false,
        }
    }
}

impl Process for FakeProcess {
    const SYSTEM_DESCRIPTION: &'static str = "This is a fake process.";

    fn spawn(&mut self, program: &str, _args: &[&str]) -> Result<String, ProcessError> {
        if self.should_fail {
            return Err(ProcessError::ProcessFailed(
                "fake process failed".to_string(),
            ));
        }

        self.spawned = Some(program.to_string());

        Ok(String::new())
    }
}
