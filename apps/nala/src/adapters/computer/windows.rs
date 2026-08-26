use crate::ports::{
    computer::{Computer, ComputerError},
    process::Process,
};

pub struct Windows<P> {
    process: P,
}

impl<P> Windows<P> {
    pub fn new(process: P) -> Self {
        Self { process }
    }
}

impl<P: Process> Computer for Windows<P> {
    fn execute_command(&mut self, command: &str) -> Result<(), ComputerError> {
        self.process
            .spawn("cmd", &["/C", command])
            .map_err(|_| ComputerError::CommandFailed)
    }
}
