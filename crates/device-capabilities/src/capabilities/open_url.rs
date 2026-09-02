use std::time::Duration;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::capability::Capability;
use crate::ports::computer::{Computer, ComputerError};

/// `start` doesn't block — it hands off to the OS and returns almost
/// instantly — so a short timeout is enough.
pub const DEFAULT_OPEN_URL_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Deserialize, JsonSchema)]
pub struct OpenUrlArgs {
    /// The URL to open in the user's default browser. Must start with
    /// `http://` or `https://`.
    pub url: String,
}

#[derive(Debug, thiserror::Error)]
pub enum OpenUrlError {
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
    #[error(transparent)]
    Computer(#[from] ComputerError),
}

pub struct OpenUrlTool<C: Computer> {
    pub computer: C,
    pub timeout: Duration,
}

impl<C: Computer> OpenUrlTool<C> {
    pub fn new(computer: C) -> Self {
        Self {
            computer,
            timeout: DEFAULT_OPEN_URL_TIMEOUT,
        }
    }

    pub fn with_timeout(computer: C, timeout: Duration) -> Self {
        Self { computer, timeout }
    }
}

fn validate_url(url: &str) -> Result<(), OpenUrlError> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(OpenUrlError::InvalidUrl(format!(
            "URL must start with http:// or https://, got: {url}"
        )));
    }

    if url.contains('"') {
        return Err(OpenUrlError::InvalidUrl(
            "URL must not contain a \" character".to_string(),
        ));
    }

    Ok(())
}

impl<C: Computer> Capability for OpenUrlTool<C> {
    type Args = OpenUrlArgs;
    type Output = String;
    type Error = OpenUrlError;

    const NAME: &'static str = "open_url";
    const DESCRIPTION: &'static str = "Open a URL in the user's default web browser. The URL must start with http:// or https://.";
    const MUTATING: bool = true;

    fn parameters() -> serde_json::Value {
        serde_json::to_value(schemars::schema_for!(OpenUrlArgs))
            .expect("OpenUrlArgs schema should serialize to JSON")
    }

    fn execute(&mut self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        validate_url(&args.url)?;

        // `rundll32 url.dll,FileProtocolHandler <url>` — the standard
        // scripted way to invoke Windows's URL protocol handler directly.
        // Both `cmd /C start "" <url>` and `explorer <url>` were observed
        // failing on a real machine (the former denied outright, the
        // latter opening File Explorer instead of the browser); this is
        // the one that reliably opened the default browser there.
        self.computer.execute_command(
            &format!("rundll32 url.dll,FileProtocolHandler \"{}\"", args.url),
            self.timeout,
        )?;

        Ok(format!("Opened {} in the default browser.", args.url))
    }

    fn parse_arguments(args: &str) -> Result<Self::Args, Self::Error> {
        serde_json::from_str(args).map_err(|error| {
            OpenUrlError::InvalidUrl(format!("could not parse arguments: {error}"))
        })
    }

    fn context(&mut self) -> Result<String, Self::Error> {
        Ok(String::new())
    }
}
