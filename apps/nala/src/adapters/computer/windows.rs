use std::fmt;

use crate::ports::{
    computer::{Computer, ComputerContext, ComputerError},
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
    type Process = P;

    const SYSTEM_DESCRIPTION: &'static str = "This is a Windows computer.";

    fn execute_command(&mut self, command: &str) -> Result<String, ComputerError> {
        self.process
            .spawn("cmd", &["/C", command])
            .map_err(|error| ComputerError::CommandFailed(format!("{error:?}")))
    }

    fn get_context(&mut self) -> Result<ComputerContext, ComputerError> {
        let username = std::env::var("USERNAME")
            .map_err(|error| ComputerError::CommandFailed(error.to_string()))?;

        let home_dir = std::env::var("USERPROFILE")
            .map_err(|error| ComputerError::CommandFailed(error.to_string()))?;

        let desktop_dir = format!(r"{home_dir}\Desktop");

        let current_dir = std::env::current_dir()
            .map_err(|error| ComputerError::CommandFailed(error.to_string()))?
            .to_string_lossy()
            .to_string();

        Ok(ComputerContext {
            system: Self::SYSTEM_DESCRIPTION.to_string(),
            username,
            home_dir,
            desktop_dir,
            current_dir,
        })
    }
}

impl fmt::Display for ComputerContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "System: {}\n\
             Username: {}\n\
             Home directory: {}\n\
             Desktop directory: {}\n\
             Current directory: {}",
            self.system, self.username, self.home_dir, self.desktop_dir, self.current_dir
        )
    }
}
