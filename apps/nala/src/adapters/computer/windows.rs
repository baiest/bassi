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
    fn excecute_command(&mut self, name: &str) -> Result<(), ComputerError> {
        self.process
            .spawn("cmd", &["/C", "start", "", name])
            .map_err(|_| ComputerError::OpenApplicationFailed)
    }
}
