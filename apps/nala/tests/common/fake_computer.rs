use nala::ports::computer::{Computer, ComputerError};

pub struct FakeComputer {
    pub opened_application: Option<String>,
    pub should_fail: bool,
}

impl FakeComputer {
    pub fn new() -> Self {
        Self {
            opened_application: None,
            should_fail: false,
        }
    }
}

impl Computer for FakeComputer {
    fn excecute_command(&mut self, name: &str) -> Result<(), ComputerError> {
        if self.should_fail {
            return Err(ComputerError::OpenApplicationFailed);
        }

        self.opened_application = Some(name.to_string());

        Ok(())
    }
}
