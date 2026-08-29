use std::fmt;
use std::time::Duration;

use crate::ports::{
    computer::{Computer, ComputerContext, ComputerError},
    environment::Environment,
    process::Process,
};

pub struct Windows<P, E> {
    process: P,
    environment: E,
}

impl<P, E> Windows<P, E> {
    pub fn new(process: P, environment: E) -> Self {
        Self {
            process,
            environment,
        }
    }
}

impl<P: Process, E: Environment> Computer for Windows<P, E> {
    type Process = P;

    const SYSTEM_DESCRIPTION: &'static str = "This is a Windows computer.";

    fn execute_command(
        &mut self,
        command: &str,
        timeout: Duration,
    ) -> Result<String, ComputerError> {
        self.process
            .spawn("cmd", &["/C", command], timeout)
            .map_err(|error| ComputerError::CommandFailed(format!("{error:?}")))
    }

    fn get_context(&mut self) -> Result<ComputerContext, ComputerError> {
        let username = self.environment.var("USERNAME")?;
        let home_dir = self.environment.var("USERPROFILE")?;
        let desktop_dir = format!(r"{home_dir}\Desktop");
        let current_dir = self.environment.current_dir()?;

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
