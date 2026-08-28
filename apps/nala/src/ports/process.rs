#[derive(Debug)]
pub enum ProcessError {
    ProcessFailed(String),
    InvalidArguments(String),
}

pub trait Process {
    const SYSTEM_DESCRIPTION: &'static str;

    fn spawn(&mut self, program: &str, args: &[&str]) -> Result<String, ProcessError>;
}
