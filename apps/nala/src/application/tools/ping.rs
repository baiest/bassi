use std::convert::Infallible;

use crate::application::tools::Tool;

/// Minimal second tool used to prove the dispatcher can route between more
/// than one tool. Takes no arguments and always succeeds.
#[derive(Default)]
pub struct PingTool;

impl PingTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for PingTool {
    type Args = ();
    type Output = String;
    type Error = Infallible;

    const NAME: &'static str = "ping";
    const DESCRIPTION: &'static str =
        "Check that the assistant's tool dispatcher is responding. Takes no arguments.";
    const PARAMETERS: &'static str = r#"{"type": "object", "properties": {}}"#;

    fn execute(&mut self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        Ok("pong".to_string())
    }

    fn parse_arguments(_arguments: &str) -> Result<Self::Args, Self::Error> {
        Ok(())
    }

    fn context(&mut self) -> Result<String, Self::Error> {
        Ok(String::new())
    }
}
