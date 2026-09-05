use std::time::Duration;

use serde::Deserialize;

use crate::capability::Capability;
use crate::ports::computer::{Computer, ComputerError};

/// PowerShell startup makes this slower than a plain `start` command, so it
/// gets a more generous timeout than `open_app`/`open_url`.
pub const DEFAULT_LIST_APPS_TIMEOUT: Duration = Duration::from_secs(15);

/// The command run to enumerate installed apps. `Get-StartApps` lists both
/// classic desktop apps and UWP/Store apps registered in the Start Menu.
/// `[Console]::OutputEncoding` is set to UTF-8 first -- without it,
/// PowerShell writes redirected stdout in the console's codepage (e.g.
/// CP1252 on a Spanish Windows), so an app name with an accented character
/// (e.g. "Configuración") comes back as invalid UTF-8 and
/// `String::from_utf8` in `adapters/process/windows.rs` rejects the whole
/// output.
const LIST_APPS_COMMAND: &str = "powershell -Command \"[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; Get-StartApps | ConvertTo-Json -Compress\"";

/// Cap on how many app names are surfaced to the model, so a machine with
/// hundreds of installed apps doesn't flood its context.
const MAX_APPS: usize = 150;

#[derive(Debug, thiserror::Error)]
pub enum ListAppsError {
    #[error(transparent)]
    Computer(#[from] ComputerError),
    #[error("could not parse Get-StartApps output: {0}")]
    Parse(String),
}

#[derive(Deserialize)]
struct StartApp {
    #[serde(rename = "Name")]
    name: String,
}

/// Parses the raw JSON from `Get-StartApps | ConvertTo-Json -Compress` into
/// a sorted, deduped, capped list of app names. `Get-StartApps` returns a
/// JSON array normally, but a single result comes back as a bare JSON
/// object instead of a one-element array, so both shapes are handled.
pub fn parse_start_apps(json: &str) -> Result<Vec<String>, String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|error| error.to_string())?;

    let entries: Vec<StartApp> = match value {
        serde_json::Value::Array(_) => {
            serde_json::from_value(value).map_err(|error| error.to_string())?
        }
        serde_json::Value::Object(_) => {
            let entry: StartApp =
                serde_json::from_value(value).map_err(|error| error.to_string())?;
            vec![entry]
        }
        _ => return Err("expected a JSON array or object".to_string()),
    };

    let mut names: Vec<String> = entries.into_iter().map(|entry| entry.name).collect();
    names.sort();
    names.dedup();

    Ok(names)
}

/// Formats the parsed names into the tool's output text, capping the list
/// and appending a truncation note when there are more than `MAX_APPS`.
fn format_output(names: &[String]) -> String {
    if names.is_empty() {
        return "No apps found.".to_string();
    }

    let total = names.len();
    let shown = &names[..total.min(MAX_APPS)];
    let mut output = shown.join("\n");

    if total > MAX_APPS {
        output.push_str(&format!(
            "\n(+{} more, ask about a specific app if you don't see it)",
            total - MAX_APPS
        ));
    }

    output
}

pub struct ListAppsTool<C: Computer> {
    pub computer: C,
    pub timeout: Duration,
}

impl<C: Computer> ListAppsTool<C> {
    pub fn new(computer: C) -> Self {
        Self {
            computer,
            timeout: DEFAULT_LIST_APPS_TIMEOUT,
        }
    }

    pub fn with_timeout(computer: C, timeout: Duration) -> Self {
        Self { computer, timeout }
    }
}

impl<C: Computer> Capability for ListAppsTool<C> {
    type Args = ();
    type Output = String;
    type Error = ListAppsError;

    const NAME: &'static str = "list_apps";
    const DESCRIPTION: &'static str = "List applications installed on the user's computer (both classic desktop apps and Store apps), by their real names as registered in the Start Menu. Call this before open_app when unsure of the exact app name.";

    fn parameters() -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }

    fn execute(&mut self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        let raw = self
            .computer
            .execute_command(LIST_APPS_COMMAND, self.timeout)?;

        let names = parse_start_apps(&raw).map_err(ListAppsError::Parse)?;

        Ok(format_output(&names))
    }

    fn parse_arguments(_arguments: &str) -> Result<Self::Args, Self::Error> {
        Ok(())
    }

    fn context(&mut self) -> Result<String, Self::Error> {
        Ok(String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_json_array_sorting_and_deduping_names() {
        let json = r#"[
            {"Name":"Notepad","AppID":"notepad.exe"},
            {"Name":"Calculator","AppID":"calc.exe"},
            {"Name":"Notepad","AppID":"notepad2.exe"}
        ]"#;

        let names = parse_start_apps(json).expect("should parse");

        assert_eq!(names, vec!["Calculator".to_string(), "Notepad".to_string()]);
    }

    #[test]
    fn parses_a_single_json_object_edge_case() {
        let json = r#"{"Name":"Notepad","AppID":"notepad.exe"}"#;

        let names = parse_start_apps(json).expect("should parse");

        assert_eq!(names, vec!["Notepad".to_string()]);
    }

    #[test]
    fn parses_an_empty_array_as_no_apps() {
        let names = parse_start_apps("[]").expect("should parse");

        assert!(names.is_empty());
    }

    #[test]
    fn rejects_malformed_json_with_a_clear_error() {
        let result = parse_start_apps("not json");

        assert!(result.is_err());
    }
}
