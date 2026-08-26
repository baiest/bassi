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
    fn spawn(&mut self, program: &str, _args: &[&str]) -> Result<(), ProcessError> {
        if self.should_fail {
            return Err(ProcessError::ProcessFailed);
        }

        self.spawned = Some(program.to_string());

        Ok(())
    }
}
