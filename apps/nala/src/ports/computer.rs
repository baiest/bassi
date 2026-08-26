#[derive(Debug)]
pub enum ComputerError {
    OpenApplicationFailed,
}

pub trait Computer {
    fn excecute_command(&mut self, name: &str) -> Result<(), ComputerError>;
}
