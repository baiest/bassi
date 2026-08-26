pub enum ProcessError {
    ProcessFailed,
}

pub trait Process {
    fn spawn(&mut self, program: &str, args: &[&str]) -> Result<(), ProcessError>;
}
