#[derive(Debug)]
pub enum ComputerError {
    CommandFailed,
}

pub trait Computer {
    fn execute_command(&mut self, command: &str) -> Result<(), ComputerError>;
}
