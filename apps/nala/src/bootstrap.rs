//! The composition root: constructs the fully-wired `Assistant` from
//! concrete adapters, so `main.rs` only has to run it. Kept as plain
//! functions rather than a builder or DI container — there's nothing here
//! that needs more than that.

use std::collections::HashMap;
use std::time::Duration;

use mcp::{ChildTransport, StdioMcpClient};
use serde::Deserialize;

#[cfg(windows)]
use crate::adapters::cancellation::console::CtrlCCancelSignal;
use crate::adapters::computer::windows::Windows;
use crate::adapters::environment::system::SystemEnvironment;
use crate::adapters::http::reqwest::ReqwestFetcher;
use crate::adapters::llm::ollama::OllamaLlm;
use crate::adapters::process::windows::Windows as WindowsProcess;
use crate::adapters::wall_clock::system::SystemWallClock;
use crate::application::assistant::Assistant;
use crate::application::tools::Tool;
use crate::application::tools::current_time::CurrentTimeTool;
use crate::application::tools::dispatcher::{ToolDispatcher, Tools};
use crate::application::tools::execute_command::ExecuteCommandTool;
use crate::application::tools::fetch_url::FetchUrlTool;
use crate::application::tools::get_weather::GetWeatherTool;
use crate::application::tools::mcp_toolset::McpToolset;
use crate::application::tools::open_app::OpenAppTool;
use crate::application::tools::open_url::OpenUrlTool;
use crate::application::tools::ping::PingTool;
use crate::application::tools::registry::ToolRegistry;
use crate::application::tools::volume::VolumeTool;
use crate::application::tools::web_search::WebSearchTool;
use crate::ports::events::EventSink;
#[cfg(windows)]
use crate::ports::llm::Llm;
#[cfg(windows)]
use crate::ports::tool_dispatcher::{ToolDispatcher as ToolDispatcherPort, ToolOutcome};

pub type ComputerType = Windows<WindowsProcess, SystemEnvironment>;
pub type McpClientType = StdioMcpClient<ChildTransport>;
pub type DispatcherType =
    ToolDispatcher<ComputerType, SystemWallClock, ReqwestFetcher, McpClientType>;

/// How long to wait for a response to a single MCP request before giving up
/// on it. Bounded so a wedged MCP server doesn't hang the turn forever.
const MCP_CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Default path (relative to the working directory) for the multi-server
/// MCP config, overridable with `NALA_MCP_CONFIG`.
const DEFAULT_MCP_CONFIG_PATH: &str = "mcp.json";

/// Used when `NALA_MODEL` isn't set. A real, commonly-pulled Ollama tag —
/// callers on a different model should set the env var rather than rely on
/// this being right for them.
pub const DEFAULT_MODEL: &str = "llama3.2";

/// Builds the fully-wired `Assistant`: the computer adapter, native tools,
/// optional MCP tools (behind `NALA_MCP=on`), and the Ollama LLM. `events`
/// is supplied by the caller rather than built here, since it differs
/// between a headless run and a voice front end that narrates through it.
pub fn build_assistant<E: EventSink>(events: E) -> Assistant<OllamaLlm, DispatcherType, E> {
    let process = WindowsProcess::new();
    let environment = SystemEnvironment::new();
    let computer = Windows::new(process, environment);

    // `Windows` isn't `Clone` (nor need it be — it's a thin, stateless
    // wrapper over `Process`/`Environment`), so each tool that needs its own
    // `Computer` gets its own cheap instance rather than sharing one.
    let open_url_computer = Windows::new(WindowsProcess::new(), SystemEnvironment::new());
    let open_app_computer = Windows::new(WindowsProcess::new(), SystemEnvironment::new());
    let volume_computer = Windows::new(WindowsProcess::new(), SystemEnvironment::new());

    let mut registry = ToolRegistry::new();
    registry.register(ExecuteCommandTool::<ComputerType>::definition());
    registry.register(OpenUrlTool::<ComputerType>::definition());
    registry.register(OpenAppTool::<ComputerType>::definition());
    registry.register(VolumeTool::<ComputerType>::definition());
    registry.register(PingTool::definition());
    registry.register(CurrentTimeTool::<SystemWallClock>::definition());
    registry.register(GetWeatherTool::<ReqwestFetcher>::definition());
    registry.register(WebSearchTool::<ReqwestFetcher>::definition());
    registry.register(FetchUrlTool::<ReqwestFetcher>::definition());

    let mut dispatcher: DispatcherType = ToolDispatcher::new();
    dispatcher.register(Tools::ExecuteCommand(ExecuteCommandTool::new(computer)));
    dispatcher.register(Tools::OpenUrl(OpenUrlTool::new(open_url_computer)));
    dispatcher.register(Tools::OpenApp(OpenAppTool::new(open_app_computer)));
    dispatcher.register(Tools::Volume(VolumeTool::new(volume_computer)));
    dispatcher.register(Tools::Ping(PingTool::new()));
    dispatcher.register(Tools::CurrentTime(CurrentTimeTool::new(
        SystemWallClock::new(),
    )));
    dispatcher.register(Tools::GetWeather(
        GetWeatherTool::new(ReqwestFetcher::new()),
    ));
    dispatcher.register(Tools::WebSearch(WebSearchTool::new(ReqwestFetcher::new())));
    dispatcher.register(Tools::FetchUrl(FetchUrlTool::new(ReqwestFetcher::new())));

    // MCP tools, spawned over stdio and exposed to the model. Servers come
    // from two places, both optional and additive:
    //   - `mcp.json` (or NALA_MCP_CONFIG) in the working directory, read
    //     unconditionally — no file means no MCP servers from this source,
    //     not an error.
    //   - the legacy single-server env vars (NALA_MCP=on plus
    //     NALA_MCP_COMMAND / NALA_MCP_TOOLS), kept for backward
    //     compatibility with existing setups; appended as one more server.
    // A server that fails to spawn or connect is skipped with a warning —
    // it never prevents the others from starting.
    let toolsets = connect_mcp_servers();
    if !toolsets.is_empty() {
        let mut seen_names = std::collections::HashSet::new();
        for toolset in &toolsets {
            for definition in toolset.definitions() {
                if seen_names.insert(definition.name.clone()) {
                    registry.register(definition.clone());
                } else {
                    eprintln!(
                        "Warning: tool '{}' is published by more than one MCP server; \
                         keeping the first one registered.",
                        definition.name
                    );
                }
            }
        }
        dispatcher.register(Tools::Mcp(toolsets));
    }

    let model = std::env::var("NALA_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());
    let llm =
        OllamaLlm::new("http://localhost:11434", &model).expect("Failed to create Ollama client");

    Assistant::new(llm, dispatcher, registry, events)
        .with_planning_enabled(std::env::var("NALA_PLANNING").as_deref() == Ok("on"))
}

/// One server entry parsed out of the MCP config JSON.
#[derive(Debug, Clone, PartialEq)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    /// `None` publishes every tool the server reports; `Some` narrows it to
    /// this allowlist.
    pub tools: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct RawMcpConfig {
    #[serde(rename = "mcpServers", default)]
    mcp_servers: HashMap<String, RawMcpServer>,
}

#[derive(Debug, Deserialize)]
struct RawMcpServer {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    tools: Option<Vec<String>>,
}

/// Parses the `mcp.json` config format:
/// `{ "mcpServers": { "name": { "command": "...", "args": [...], "tools": [...] } } }`.
/// Pure and side-effect-free so it can be tested without spawning real
/// processes. `tools` is optional; when absent, every tool the server
/// reports gets published.
pub fn parse_mcp_config(json: &str) -> Result<Vec<McpServerConfig>, String> {
    let raw: RawMcpConfig = serde_json::from_str(json).map_err(|error| error.to_string())?;

    Ok(raw
        .mcp_servers
        .into_iter()
        .map(|(name, server)| McpServerConfig {
            name,
            command: server.command,
            args: server.args,
            tools: server.tools,
        })
        .collect())
}

/// Reads and parses the MCP config file at `NALA_MCP_CONFIG` (or
/// `mcp.json` in the working directory if unset). A missing file is not an
/// error — Nala just runs without servers from this source.
fn read_mcp_config_file() -> Vec<McpServerConfig> {
    let path =
        std::env::var("NALA_MCP_CONFIG").unwrap_or_else(|_| DEFAULT_MCP_CONFIG_PATH.to_string());

    let Ok(contents) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };

    match parse_mcp_config(&contents) {
        Ok(servers) => servers,
        Err(error) => {
            eprintln!("Warning: could not parse MCP config '{path}' ({error}); ignoring it.");
            Vec::new()
        }
    }
}

/// The legacy single-server config from env vars, appended as one more
/// server so existing `NALA_MCP=on` setups keep working. `None` when
/// `NALA_MCP` isn't `on`, or when `NALA_MCP_COMMAND` is missing/empty.
fn legacy_env_mcp_server() -> Option<McpServerConfig> {
    if std::env::var("NALA_MCP").as_deref() != Ok("on") {
        return None;
    }

    let command = std::env::var("NALA_MCP_COMMAND").ok()?;
    let mut parts = command.split_whitespace();
    let program = parts.next()?.to_string();
    let args: Vec<String> = parts.map(str::to_string).collect();

    let tools: Option<Vec<String>> = std::env::var("NALA_MCP_TOOLS").ok().map(|value| {
        value
            .split(',')
            .map(|name| name.trim().to_string())
            .collect()
    });

    Some(McpServerConfig {
        name: "legacy-env".to_string(),
        command: program,
        args,
        tools,
    })
}

/// Spawns every configured MCP server and connects a toolset to each one.
/// A server that fails to spawn or connect is skipped with a warning
/// instead of aborting the rest.
fn connect_mcp_servers() -> Vec<McpToolset<McpClientType>> {
    let mut configs = read_mcp_config_file();
    configs.extend(legacy_env_mcp_server());

    configs
        .into_iter()
        .filter_map(|config| match connect_mcp_server(&config) {
            Ok(toolset) => Some(toolset),
            Err(error) => {
                eprintln!(
                    "Warning: could not start MCP server '{}' ({error}); skipping it.",
                    config.name
                );
                None
            }
        })
        .collect()
}

fn connect_mcp_server(config: &McpServerConfig) -> Result<McpToolset<McpClientType>, String> {
    let args: Vec<&str> = config.args.iter().map(String::as_str).collect();
    let transport = ChildTransport::spawn(&config.command, &args, MCP_CALL_TIMEOUT)
        .map_err(|error| error.to_string())?;
    let client = StdioMcpClient::new(transport);

    let allowed_refs: Option<Vec<&str>> = config
        .tools
        .as_ref()
        .map(|names| names.iter().map(String::as_str).collect());

    McpToolset::connect(client, allowed_refs.as_deref()).map_err(|error| error.to_string())
}

/// Installs the Ctrl+C-cancels-the-turn handler and wires it into
/// `assistant`, returning both the (possibly updated) assistant and the
/// signal so the caller can `reset()` it once per turn. Windows only — this
/// whole integration is a `SetConsoleCtrlHandler` wrapper that doesn't
/// exist on other platforms. Installation failure is non-fatal: it just
/// means Ctrl+C won't cancel a turn, so the caller gets `None` back.
#[cfg(windows)]
pub fn install_cancel_signal<L, D, E>(
    assistant: Assistant<L, D, E>,
) -> (Assistant<L, D, E>, Option<CtrlCCancelSignal>)
where
    L: Llm + Send + 'static,
    D: ToolDispatcherPort<Output = ToolOutcome>,
    D::Error: std::error::Error + 'static,
    E: EventSink,
{
    match CtrlCCancelSignal::install() {
        Ok(signal) => {
            let assistant = assistant.with_cancel_signal(Box::new(signal.clone()));
            (assistant, Some(signal))
        }
        Err(error) => {
            eprintln!(
                "Warning: could not install Ctrl+C handler ({error}); \
                 Ctrl+C during a turn will not cancel it."
            );
            (assistant, None)
        }
    }
}

#[cfg(not(windows))]
pub fn install_cancel_signal<L, D, E>(
    assistant: Assistant<L, D, E>,
) -> (Assistant<L, D, E>, Option<()>) {
    (assistant, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multiple_servers_with_and_without_an_allowlist() {
        let json = r#"{
            "mcpServers": {
                "one": { "command": "npx", "args": ["-y", "server-one"], "tools": ["a", "b"] },
                "two": { "command": "python", "args": ["server_two.py"] }
            }
        }"#;

        let mut servers = parse_mcp_config(json).expect("valid config should parse");
        servers.sort_by(|a, b| a.name.cmp(&b.name));

        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].name, "one");
        assert_eq!(servers[0].command, "npx");
        assert_eq!(servers[0].args, vec!["-y", "server-one"]);
        assert_eq!(
            servers[0].tools,
            Some(vec!["a".to_string(), "b".to_string()])
        );

        assert_eq!(servers[1].name, "two");
        assert_eq!(servers[1].tools, None);
    }

    #[test]
    fn parses_a_config_with_no_servers() {
        let servers = parse_mcp_config(r#"{ "mcpServers": {} }"#).expect("should parse");

        assert!(servers.is_empty());
    }

    #[test]
    fn rejects_invalid_json() {
        let result = parse_mcp_config("not json");

        assert!(result.is_err());
    }

    #[test]
    fn missing_mcp_servers_key_yields_no_servers() {
        let servers = parse_mcp_config("{}").expect("missing key should default to empty");

        assert!(servers.is_empty());
    }
}
