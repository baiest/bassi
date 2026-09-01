use std::time::Duration;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::capability::Capability;
use crate::ports::computer::{Computer, ComputerError};

/// A short, fire-and-forget PowerShell invocation — 10s is generous.
pub const DEFAULT_VOLUME_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Deserialize, JsonSchema)]
pub struct VolumeArgs {
    /// The volume action to perform: "up", "down", or "mute". "mute" is a
    /// toggle — call it again to unmute, there is no separate "unmute"
    /// action.
    pub action: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VolumeAction {
    Up,
    Down,
    Mute,
}

impl VolumeAction {
    fn parse(value: &str) -> Result<Self, VolumeError> {
        match value {
            "up" => Ok(Self::Up),
            "down" => Ok(Self::Down),
            "mute" => Ok(Self::Mute),
            other => Err(VolumeError::InvalidAction(format!(
                "unknown volume action: {other}. Expected one of \"up\", \"down\", \"mute\"."
            ))),
        }
    }

    /// The classic Windows SendKeys media-key trick via PowerShell — no
    /// COM/P-Invoke crate needed. [char]175 = volume up, 174 = volume down,
    /// 173 = mute (toggle).
    fn command(self) -> &'static str {
        match self {
            Self::Up => {
                r#"powershell -Command "(New-Object -ComObject WScript.Shell).SendKeys([char]175)""#
            }
            Self::Down => {
                r#"powershell -Command "(New-Object -ComObject WScript.Shell).SendKeys([char]174)""#
            }
            Self::Mute => {
                r#"powershell -Command "(New-Object -ComObject WScript.Shell).SendKeys([char]173)""#
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VolumeError {
    #[error("invalid action: {0}")]
    InvalidAction(String),
    #[error(transparent)]
    Computer(#[from] ComputerError),
}

pub struct VolumeTool<C: Computer> {
    pub computer: C,
    pub timeout: Duration,
}

impl<C: Computer> VolumeTool<C> {
    pub fn new(computer: C) -> Self {
        Self {
            computer,
            timeout: DEFAULT_VOLUME_TIMEOUT,
        }
    }

    pub fn with_timeout(computer: C, timeout: Duration) -> Self {
        Self { computer, timeout }
    }
}

impl<C: Computer> Capability for VolumeTool<C> {
    type Args = VolumeArgs;
    type Output = String;
    type Error = VolumeError;

    const NAME: &'static str = "volume";
    const DESCRIPTION: &'static str = "Change the system volume: \"up\" or \"down\" step the volume, \"mute\" toggles mute (call it again to unmute — there is no separate \"unmute\" action).";
    const MUTATING: bool = true;

    fn parameters() -> serde_json::Value {
        serde_json::to_value(schemars::schema_for!(VolumeArgs))
            .expect("VolumeArgs schema should serialize to JSON")
    }

    fn execute(&mut self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let action = VolumeAction::parse(&args.action)?;

        self.computer
            .execute_command(action.command(), self.timeout)?;

        Ok(format!("Volume action '{}' sent.", args.action))
    }

    fn parse_arguments(args: &str) -> Result<Self::Args, Self::Error> {
        serde_json::from_str(args).map_err(|error| {
            VolumeError::InvalidAction(format!("could not parse arguments: {error}"))
        })
    }

    fn context(&mut self) -> Result<String, Self::Error> {
        Ok(String::new())
    }
}
