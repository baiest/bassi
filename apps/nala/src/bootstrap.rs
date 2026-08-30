//! The composition root: constructs the fully-wired `Assistant` from
//! concrete adapters, so `main.rs` only has to run it. Kept as plain
//! functions rather than a builder or DI container — there's nothing here
//! that needs more than that.

use std::time::Duration;

use mcp::{ChildTransport, StdioMcpClient};

#[cfg(windows)]
use crate::adapters::cancellation::console::CtrlCCancelSignal;
use crate::adapters::computer::windows::Windows;
use crate::adapters::environment::system::SystemEnvironment;
use crate::adapters::llm::ollama::OllamaLlm;
use crate::adapters::process::windows::Windows as WindowsProcess;
use crate::application::assistant::Assistant;
use crate::application::tools::Tool;
use crate::application::tools::dispatcher::{ToolDispatcher, Tools};
use crate::application::tools::execute_command::ExecuteCommandTool;
use crate::application::tools::mcp_toolset::McpToolset;
use crate::application::tools::ping::PingTool;
use crate::application::tools::registry::ToolRegistry;
use crate::ports::events::EventSink;
#[cfg(windows)]
use crate::ports::llm::Llm;
#[cfg(windows)]
use crate::ports::tool_dispatcher::{ToolDispatcher as ToolDispatcherPort, ToolOutcome};

pub type ComputerType = Windows<WindowsProcess, SystemEnvironment>;
pub type McpClientType = StdioMcpClient<ChildTransport>;

/// How long to wait for a response to a single MCP request before giving up
/// on it. Bounded so a wedged MCP server doesn't hang the turn forever.
const MCP_CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Used when `NALA_MODEL` isn't set. A real, commonly-pulled Ollama tag —
/// callers on a different model should set the env var rather than rely on
/// this being right for them.
pub const DEFAULT_MODEL: &str = "llama3.2";

/// Builds the fully-wired `Assistant`: the computer adapter, native tools,
/// optional MCP tools (behind `NALA_MCP=on`), and the Ollama LLM. `events`
/// is supplied by the caller rather than built here, since it differs
/// between a headless run and a voice front end that narrates through it.
pub fn build_assistant<E: EventSink>(
    events: E,
) -> Assistant<OllamaLlm, ToolDispatcher<ComputerType, McpClientType>, E> {
    let process = WindowsProcess::new();
    let environment = SystemEnvironment::new();
    let computer = Windows::new(process, environment);

    let mut registry = ToolRegistry::new();
    registry.register(ExecuteCommandTool::<ComputerType>::definition());
    registry.register(PingTool::definition());

    let mut dispatcher: ToolDispatcher<ComputerType, McpClientType> = ToolDispatcher::new();
    dispatcher.register(Tools::ExecuteCommand(ExecuteCommandTool::new(computer)));
    dispatcher.register(Tools::Ping(PingTool::new()));

    // MCP tools, spawned over stdio and exposed to the model. Disabled by
    // default — Nala runs with its native tools alone. Set NALA_MCP=on plus
    // NALA_MCP_COMMAND (e.g. "npx -y some-mcp-server") to opt in; narrow
    // which of the server's tools get published with NALA_MCP_TOOLS
    // (comma-separated), or leave it unset to publish all of them.
    if std::env::var("NALA_MCP").as_deref() == Ok("on") {
        match connect_mcp() {
            Ok(toolset) => {
                for definition in toolset.definitions() {
                    registry.register(definition.clone());
                }
                dispatcher.register(Tools::Mcp(toolset));
            }
            Err(error) => {
                eprintln!(
                    "Warning: could not start the MCP server ({error}); \
                     Nala will run without its tools."
                );
            }
        }
    }

    let model = std::env::var("NALA_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());
    let llm =
        OllamaLlm::new("http://localhost:11434", &model).expect("Failed to create Ollama client");

    Assistant::new(llm, dispatcher, registry, events)
        .with_planning_enabled(std::env::var("NALA_PLANNING").as_deref() == Ok("on"))
}

fn connect_mcp() -> Result<McpToolset<McpClientType>, String> {
    let command =
        std::env::var("NALA_MCP_COMMAND").map_err(|_| "NALA_MCP_COMMAND is not set".to_string())?;

    let mut parts = command.split_whitespace();
    let program = parts
        .next()
        .ok_or_else(|| "NALA_MCP_COMMAND is empty".to_string())?;
    let args: Vec<&str> = parts.collect();

    let transport = ChildTransport::spawn(program, &args, MCP_CALL_TIMEOUT)
        .map_err(|error| error.to_string())?;
    let client = StdioMcpClient::new(transport);

    let allowed: Option<Vec<String>> = std::env::var("NALA_MCP_TOOLS").ok().map(|value| {
        value
            .split(',')
            .map(|name| name.trim().to_string())
            .collect()
    });
    let allowed_refs: Option<Vec<&str>> = allowed
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
