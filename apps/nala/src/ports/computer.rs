#[derive(Debug)]
pub enum ComputerError {
    OpenApplicationFailed,
}

pub trait Computer {
    fn open_application(&mut self, name: &str) -> Result<(), ComputerError>;
}
