use std::time::Duration;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::capability::Capability;
use crate::ports::computer::{Computer, ComputerError};

/// `start` doesn't block — it hands off to the OS and returns almost
/// instantly — so a short timeout is enough.
pub const DEFAULT_OPEN_APP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Deserialize, JsonSchema)]
pub struct OpenAppArgs {
    /// The application to open, e.g. "notepad", "calc", "explorer", or a
    /// full path to an executable.
    pub app: String,
}

#[derive(Debug, thiserror::Error)]
pub enum OpenAppError {
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error(transparent)]
    Computer(#[from] ComputerError),
}

pub struct OpenAppTool<C: Computer> {
    pub computer: C,
    pub timeout: Duration,
}

impl<C: Computer> OpenAppTool<C> {
    pub fn new(computer: C) -> Self {
        Self {
            computer,
            timeout: DEFAULT_OPEN_APP_TIMEOUT,
        }
    }

    pub fn with_timeout(computer: C, timeout: Duration) -> Self {
        Self { computer, timeout }
    }
}

fn validate_app(app: &str) -> Result<(), OpenAppError> {
    if app.trim().is_empty() {
        return Err(OpenAppError::InvalidArgument(
            "app must not be empty".to_string(),
        ));
    }

    if app.contains('"') {
        return Err(OpenAppError::InvalidArgument(
            "app must not contain a \" character".to_string(),
        ));
    }

    Ok(())
}

impl<C: Computer> Capability for OpenAppTool<C> {
    type Args = OpenAppArgs;
    type Output = String;
    type Error = OpenAppError;

    const NAME: &'static str = "open_app";
    const DESCRIPTION: &'static str = "Open an application on the user's computer, by name (e.g. \"notepad\", \"calc\", \"explorer\") or full path.";
    const MUTATING: bool = true;

    fn parameters() -> serde_json::Value {
        serde_json::to_value(schemars::schema_for!(OpenAppArgs))
            .expect("OpenAppArgs schema should serialize to JSON")
    }

    fn execute(&mut self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        validate_app(&args.app)?;

        self.computer
            .execute_command(&format!("start \"\" \"{}\"", args.app), self.timeout)?;

        Ok(format!("Opened {}.", args.app))
    }

    fn parse_arguments(args: &str) -> Result<Self::Args, Self::Error> {
        serde_json::from_str(args).map_err(|error| {
            OpenAppError::InvalidArgument(format!("could not parse arguments: {error}"))
        })
    }

    fn context(&mut self) -> Result<String, Self::Error> {
        Ok(String::new())
    }
}
