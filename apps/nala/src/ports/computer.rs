#[derive(Debug)]
pub enum ComputerError {
    OpenApplicationFailed,
}

pub trait Computer {
    fn execute_command(&mut self, name: &str) -> Result<(), ComputerError>;
}
