use std::convert::Infallible;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::application::tools::Tool;
use crate::ports::wall_clock::WallClock;

#[derive(Deserialize, JsonSchema)]
pub struct CurrentTimeArgs {}

pub struct CurrentTimeTool<C: WallClock> {
    clock: C,
}

impl<C: WallClock> CurrentTimeTool<C> {
    pub fn new(clock: C) -> Self {
        Self { clock }
    }
}

impl<C: WallClock> Tool for CurrentTimeTool<C> {
    type Args = ();
    type Output = String;
    type Error = Infallible;

    const NAME: &'static str = "current_time";
    const DESCRIPTION: &'static str = "Get the current local date and time. Takes no arguments.";

    fn parameters() -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }

    fn execute(&mut self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        let now = self.clock.now_local();

        let months = [
            "enero",
            "febrero",
            "marzo",
            "abril",
            "mayo",
            "junio",
            "julio",
            "agosto",
            "septiembre",
            "octubre",
            "noviembre",
            "diciembre",
        ];
        let month =
            months[(now.format("%m").to_string().parse::<usize>().unwrap_or(1) - 1).clamp(0, 11)];

        Ok(format!(
            "Son las {} del {} de {} de {}.",
            now.format("%H:%M"),
            now.format("%-d"),
            month,
            now.format("%Y"),
        ))
    }

    fn parse_arguments(_arguments: &str) -> Result<Self::Args, Self::Error> {
        Ok(())
    }

    fn context(&mut self) -> Result<String, Self::Error> {
        Ok(String::new())
    }
}
