use chrono::{DateTime, Local};
use device_capabilities::Capability;
use device_capabilities::capabilities::execute_command::ExecuteCommandTool;
use device_capabilities::capabilities::list_apps::ListAppsTool;
use device_capabilities::capabilities::open_app::OpenAppTool;
use device_capabilities::capabilities::open_url::OpenUrlTool;
use device_capabilities::capabilities::volume::VolumeTool;
use device_capabilities::ports::computer::Computer;
use device_protocol::{CapabilityDefinition, Outcome};
use mcp::McpClient;

use std::sync::Arc;

use crate::{
    application::{
        devices::registry::DeviceRegistry,
        tools::{
            Tool, current_time::CurrentTimeTool, device_toolset::DeviceToolset,
            fetch_url::FetchUrlTool, get_weather::GetWeatherTool, mcp_toolset::McpToolset,
            ping::PingTool, remember::RememberTool, web_search::WebSearchTool,
        },
    },
    ports::{
        device::RemoteDevice,
        http::{HttpError, HttpFetcher},
        llm::ToolCall,
        tool_dispatcher::{ToolDispatcher as ToolDispatcherPort, ToolOutcome},
        wall_clock::WallClock,
    },
};

type BoxedError = Box<dyn std::error::Error + Send + Sync + 'static>;

#[derive(Debug, thiserror::Error)]
pub enum ToolDispatcherError {
    #[error("tool not found")]
    ToolNotFound,
    #[error("failed to parse tool call arguments: {0}")]
    ToolErrorParsingArguments(#[source] BoxedError),
    #[error("tool execution failed: {0}")]
    ToolExecuteError(#[source] BoxedError),
}

/// A `McpClient` that is never actually connected to anything — the default
/// for `M` so callers that don't use `Tools::Mcp` (most tests, and any
/// setup with `NALA_MCP` off) don't have to name an MCP client type at all.
#[derive(Debug, Default)]
pub struct NoMcpClient;

impl McpClient for NoMcpClient {
    fn list_tools(&mut self) -> Result<Vec<mcp::McpToolInfo>, mcp::McpError> {
        Ok(Vec::new())
    }

    fn call_tool(
        &mut self,
        _name: &str,
        _arguments: serde_json::Value,
    ) -> Result<mcp::McpToolResult, mcp::McpError> {
        Err(mcp::McpError::Transport(
            "NoMcpClient is never connected".to_string(),
        ))
    }
}

/// A `WallClock` that never gets used — the default for `CL` so callers
/// that don't register a `current_time` tool (most tests) don't have to
/// name a wall-clock type at all.
#[derive(Debug, Default)]
pub struct NoWallClock;

impl WallClock for NoWallClock {
    fn now_local(&self) -> DateTime<Local> {
        Local::now()
    }
}

/// An `HttpFetcher` that always fails — the default for `H` so callers that
/// don't register any of the HTTP-backed tools (most tests) don't have to
/// name an HTTP client type at all.
#[derive(Debug, Default)]
pub struct NoHttpFetcher;

impl HttpFetcher for NoHttpFetcher {
    fn get(&self, _url: &str) -> Result<String, HttpError> {
        Err(HttpError::Request(
            "NoHttpFetcher is never connected".to_string(),
        ))
    }
}

/// A `RemoteDevice` that is never actually connected — the default for `R`
/// so callers that don't use `Tools::Devices` (most tests, and any Nala
/// with no devices attached) don't have to name a device type at all.
#[derive(Debug, Default, Clone)]
pub struct NoDevice;

impl RemoteDevice for NoDevice {
    fn name(&self) -> &str {
        "no-device"
    }

    fn capabilities(&self) -> &[CapabilityDefinition] {
        &[]
    }

    fn invoke(&mut self, _capability: &str, _arguments: &str) -> Outcome {
        Outcome::Err {
            code: device_protocol::ErrorCode::NotFound,
            message: "NoDevice is never connected".to_string(),
        }
    }
}

/// One variant per `Tool` implementation the dispatcher knows how to run.
/// Adding a native tool means adding a variant here and a match arm below —
/// both checked exhaustively at compile time, no runtime type erasure. MCP
/// tools all share one variant, since their identities are only known once
/// connected to a server; devices work the same way, one `DeviceToolset`
/// per connected device.
pub enum Tools<
    C: Computer,
    CL: WallClock = NoWallClock,
    H: HttpFetcher = NoHttpFetcher,
    M: McpClient = NoMcpClient,
    R: RemoteDevice = NoDevice,
> {
    ExecuteCommand(ExecuteCommandTool<C>),
    OpenUrl(OpenUrlTool<C>),
    OpenApp(OpenAppTool<C>),
    Volume(VolumeTool<C>),
    ListApps(ListAppsTool<C>),
    Ping(PingTool),
    CurrentTime(CurrentTimeTool<CL>),
    GetWeather(GetWeatherTool<H>),
    WebSearch(WebSearchTool<H>),
    FetchUrl(FetchUrlTool<H>),
    Remember(RememberTool),
    // One `McpToolset` per connected server, so a tool-name collision
    // across servers is resolved by taking the first one that handles it —
    // see `dispatch` below.
    Mcp(Vec<McpToolset<M>>),
    // Same pattern, one `DeviceToolset` per connected device.
    Devices(Vec<DeviceToolset<R>>),
}

pub struct ToolDispatcher<
    C: Computer,
    CL: WallClock = NoWallClock,
    H: HttpFetcher = NoHttpFetcher,
    M: McpClient = NoMcpClient,
    R: RemoteDevice + Clone = NoDevice,
> {
    tools: Vec<Tools<C, CL, H, M, R>>,
    /// The live source of truth for connected devices, when one is wired
    /// up (`with_device_registry`). Re-snapshotted on every `dispatch` /
    /// `device_tools` call (see `sync_devices`) rather than once at
    /// construction, so a device connecting or disconnecting mid-session is
    /// picked up without the turn-client reconnecting. `None` for callers
    /// with no device server at all (the local REPL, most tests) — devices
    /// registered directly via `Tools::Devices` still work either way.
    device_registry: Option<Arc<DeviceRegistry<R>>>,
}

impl<C: Computer, CL: WallClock, H: HttpFetcher, M: McpClient, R: RemoteDevice + Clone>
    ToolDispatcher<C, CL, H, M, R>
{
    pub fn new() -> Self {
        Self {
            tools: Vec::new(),
            device_registry: None,
        }
    }

    pub fn register(&mut self, tool: Tools<C, CL, H, M, R>) {
        self.tools.push(tool);
    }

    /// Wires a live `DeviceRegistry` in, so connected devices are picked up
    /// as they come and go instead of being fixed at construction time.
    pub fn with_device_registry(mut self, registry: Arc<DeviceRegistry<R>>) -> Self {
        self.device_registry = Some(registry);
        self
    }

    /// Replaces every `Tools::Devices` entry with one built fresh from the
    /// registry's current snapshot. A no-op when no registry is wired up.
    fn sync_devices(&mut self) {
        let Some(registry) = &self.device_registry else {
            return;
        };

        self.tools.retain(|tool| !matches!(tool, Tools::Devices(_)));

        let devices = registry.snapshot();
        if !devices.is_empty() {
            self.tools.push(Tools::Devices(
                devices.into_iter().map(DeviceToolset::new).collect(),
            ));
        }
    }

    /// Routes `tool_call` to a connected device when one handles it — either
    /// its exact prefixed name (`pc_open_url`) or, when exactly one device
    /// publishes the bare capability (`open_url`), that name too. With more
    /// than one device publishing the same bare capability there's no way
    /// to tell which the model meant, so it's left unhandled here and falls
    /// through to the native tool (which still requires the prefix to reach
    /// a specific device).
    fn dispatch_to_device(&mut self, tool_call: &ToolCall) -> Option<ToolOutcome> {
        for tool in &mut self.tools {
            let Tools::Devices(toolsets) = tool else {
                continue;
            };

            if let Some(toolset) = toolsets
                .iter_mut()
                .find(|toolset| toolset.handles(&tool_call.name))
            {
                return Some(toolset.call(&tool_call.name, &tool_call.arguments));
            }

            let mut bare_matches: Vec<usize> = toolsets
                .iter()
                .enumerate()
                .filter(|(_, toolset)| toolset.handles_bare(&tool_call.name))
                .map(|(index, _)| index)
                .collect();

            if bare_matches.len() == 1 {
                let index = bare_matches.remove(0);
                return Some(toolsets[index].call(&tool_call.name, &tool_call.arguments));
            }

            return None;
        }

        None
    }
}

impl<C: Computer, CL: WallClock, H: HttpFetcher, M: McpClient, R: RemoteDevice + Clone> Default
    for ToolDispatcher<C, CL, H, M, R>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<C: Computer, CL: WallClock, H: HttpFetcher, M: McpClient, R: RemoteDevice + Clone>
    ToolDispatcherPort for ToolDispatcher<C, CL, H, M, R>
{
    type Output = ToolOutcome;
    type Error = ToolDispatcherError;

    fn dispatch(&mut self, tool_call: ToolCall) -> Result<Self::Output, Self::Error> {
        self.sync_devices();

        // A connected device always wins over a native tool doing the same
        // thing — checked before the native arms below, not after, so a
        // bare name (`open_url`) reaches the device the user meant instead
        // of running locally in this process. See `dispatch_to_device`.
        if let Some(outcome) = self.dispatch_to_device(&tool_call) {
            return Ok(outcome);
        }

        for tool in &mut self.tools {
            match tool {
                Tools::ExecuteCommand(tool) if tool_call.name == ExecuteCommandTool::<C>::NAME => {
                    let args = ExecuteCommandTool::<C>::parse_arguments(&tool_call.arguments)
                        .map_err(|error| {
                            ToolDispatcherError::ToolErrorParsingArguments(Box::new(error))
                        })?;

                    // Best-effort: a `before`/`after` snapshot of the
                    // computer's state, so the model doesn't have to take
                    // our word for it that the command changed anything —
                    // a failure to read either snapshot just means running
                    // without that evidence, not a failed command.
                    let before = tool.context().ok();

                    let output = tool
                        .execute(args)
                        .map_err(|error| ToolDispatcherError::ToolExecuteError(Box::new(error)))?;

                    let after = tool.context().ok();

                    let text = match (before, after) {
                        (Some(before), Some(after)) if before != after => {
                            format!("{output}\n\nState before:\n{before}\n\nState after:\n{after}")
                        }
                        (Some(_), Some(after)) => {
                            format!("{output}\n\nState unchanged:\n{after}")
                        }
                        _ => output,
                    };

                    return Ok(ToolOutcome {
                        text,
                        images: Vec::new(),
                        mutated: ExecuteCommandTool::<C>::MUTATING,
                    });
                }
                Tools::OpenUrl(tool) if tool_call.name == OpenUrlTool::<C>::NAME => {
                    let args = OpenUrlTool::<C>::parse_arguments(&tool_call.arguments).map_err(
                        |error| ToolDispatcherError::ToolErrorParsingArguments(Box::new(error)),
                    )?;

                    let text = tool
                        .execute(args)
                        .map_err(|error| ToolDispatcherError::ToolExecuteError(Box::new(error)))?;

                    return Ok(ToolOutcome {
                        text,
                        images: Vec::new(),
                        mutated: OpenUrlTool::<C>::MUTATING,
                    });
                }
                Tools::OpenApp(tool) if tool_call.name == OpenAppTool::<C>::NAME => {
                    let args = OpenAppTool::<C>::parse_arguments(&tool_call.arguments).map_err(
                        |error| ToolDispatcherError::ToolErrorParsingArguments(Box::new(error)),
                    )?;

                    let text = tool
                        .execute(args)
                        .map_err(|error| ToolDispatcherError::ToolExecuteError(Box::new(error)))?;

                    return Ok(ToolOutcome {
                        text,
                        images: Vec::new(),
                        mutated: OpenAppTool::<C>::MUTATING,
                    });
                }
                Tools::Volume(tool) if tool_call.name == VolumeTool::<C>::NAME => {
                    let args = VolumeTool::<C>::parse_arguments(&tool_call.arguments).map_err(
                        |error| ToolDispatcherError::ToolErrorParsingArguments(Box::new(error)),
                    )?;

                    let text = tool
                        .execute(args)
                        .map_err(|error| ToolDispatcherError::ToolExecuteError(Box::new(error)))?;

                    return Ok(ToolOutcome {
                        text,
                        images: Vec::new(),
                        mutated: VolumeTool::<C>::MUTATING,
                    });
                }
                Tools::ListApps(tool) if tool_call.name == ListAppsTool::<C>::NAME => {
                    ListAppsTool::<C>::parse_arguments(&tool_call.arguments).map_err(|error| {
                        ToolDispatcherError::ToolErrorParsingArguments(Box::new(error))
                    })?;

                    return tool
                        .execute(())
                        .map(ToolOutcome::from)
                        .map_err(|error| ToolDispatcherError::ToolExecuteError(Box::new(error)));
                }
                Tools::Ping(tool) if tool_call.name == PingTool::NAME => {
                    PingTool::parse_arguments(&tool_call.arguments).map_err(|error| {
                        ToolDispatcherError::ToolErrorParsingArguments(Box::new(error))
                    })?;

                    return tool
                        .execute(())
                        .map(ToolOutcome::from)
                        .map_err(|error| ToolDispatcherError::ToolExecuteError(Box::new(error)));
                }
                Tools::CurrentTime(tool) if tool_call.name == CurrentTimeTool::<CL>::NAME => {
                    CurrentTimeTool::<CL>::parse_arguments(&tool_call.arguments).map_err(
                        |error| ToolDispatcherError::ToolErrorParsingArguments(Box::new(error)),
                    )?;

                    return tool
                        .execute(())
                        .map(ToolOutcome::from)
                        .map_err(|error| ToolDispatcherError::ToolExecuteError(Box::new(error)));
                }
                Tools::GetWeather(tool) if tool_call.name == GetWeatherTool::<H>::NAME => {
                    let args = GetWeatherTool::<H>::parse_arguments(&tool_call.arguments).map_err(
                        |error| ToolDispatcherError::ToolErrorParsingArguments(Box::new(error)),
                    )?;

                    return tool
                        .execute(args)
                        .map(ToolOutcome::from)
                        .map_err(|error| ToolDispatcherError::ToolExecuteError(Box::new(error)));
                }
                Tools::WebSearch(tool) if tool_call.name == WebSearchTool::<H>::NAME => {
                    let args = WebSearchTool::<H>::parse_arguments(&tool_call.arguments).map_err(
                        |error| ToolDispatcherError::ToolErrorParsingArguments(Box::new(error)),
                    )?;

                    return tool
                        .execute(args)
                        .map(ToolOutcome::from)
                        .map_err(|error| ToolDispatcherError::ToolExecuteError(Box::new(error)));
                }
                Tools::FetchUrl(tool) if tool_call.name == FetchUrlTool::<H>::NAME => {
                    let args = FetchUrlTool::<H>::parse_arguments(&tool_call.arguments).map_err(
                        |error| ToolDispatcherError::ToolErrorParsingArguments(Box::new(error)),
                    )?;

                    return tool
                        .execute(args)
                        .map(ToolOutcome::from)
                        .map_err(|error| ToolDispatcherError::ToolExecuteError(Box::new(error)));
                }
                Tools::Remember(tool) if tool_call.name == RememberTool::NAME => {
                    let args =
                        RememberTool::parse_arguments(&tool_call.arguments).map_err(|error| {
                            ToolDispatcherError::ToolErrorParsingArguments(Box::new(error))
                        })?;

                    let text = tool
                        .execute(args)
                        .map_err(|error| ToolDispatcherError::ToolExecuteError(Box::new(error)))?;

                    return Ok(ToolOutcome {
                        text,
                        images: Vec::new(),
                        mutated: RememberTool::MUTATING,
                    });
                }
                Tools::Mcp(toolsets) => {
                    let Some(toolset) = toolsets
                        .iter_mut()
                        .find(|toolset| toolset.handles(&tool_call.name))
                    else {
                        continue;
                    };

                    let result = toolset
                        .call(&tool_call.name, &tool_call.arguments)
                        .map_err(|error| ToolDispatcherError::ToolExecuteError(Box::new(error)))?;

                    return Ok(ToolOutcome {
                        text: result.text,
                        images: result.images,
                        // MCP's protocol has no way to say whether a call
                        // changed anything server-side, so it never gates
                        // on `mutated` — the verification gate only
                        // applies to native tools that declare `MUTATING`.
                        mutated: false,
                    });
                }
                _ => continue,
            }
        }

        Err(ToolDispatcherError::ToolNotFound)
    }

    fn device_tools(&mut self) -> Vec<crate::ports::tool::ToolDefinition> {
        self.sync_devices();

        // `device_toolset.rs::DeviceToolset::definitions()` returns owned
        // `ToolDefinition`s, so this just concatenates each connected
        // device's list — no snapshot staleness, since `sync_devices` just
        // rebuilt `self.tools`'s device layer from the live registry.
        let mut definitions = Vec::new();
        for tool in &self.tools {
            if let Tools::Devices(toolsets) = tool {
                for toolset in toolsets {
                    definitions.extend(toolset.definitions());
                }
            }
        }
        definitions
    }

    fn get_context(&mut self) -> Result<String, Self::Error> {
        for tool in &mut self.tools {
            if let Tools::ExecuteCommand(tool) = tool {
                return tool
                    .context()
                    .map_err(|error| ToolDispatcherError::ToolExecuteError(Box::new(error)));
            }
        }

        Err(ToolDispatcherError::ToolNotFound)
    }
}

#[cfg(test)]
mod device_routing_tests {
    use super::*;
    use crate::application::tools::ping::PingTool;
    use device_capabilities::adapters::computer::windows::Windows;
    use device_capabilities::adapters::environment::system::SystemEnvironment;
    use device_capabilities::adapters::process::windows::Windows as WindowsProcess;

    type TestComputer = Windows<WindowsProcess, SystemEnvironment>;

    /// A `RemoteDevice` that always answers `Outcome::Ok` naming itself, so
    /// a test can tell whether a call reached the device (vs. the native
    /// tool, which would instead go through `PingTool::execute`).
    #[derive(Clone)]
    struct FakeDevice {
        device_name: &'static str,
        capabilities: Vec<CapabilityDefinition>,
    }

    impl FakeDevice {
        fn publishing(device_name: &'static str, capability: &str) -> Self {
            Self {
                device_name,
                capabilities: vec![CapabilityDefinition {
                    name: capability.to_string(),
                    description: String::new(),
                    parameters: serde_json::json!({}),
                }],
            }
        }
    }

    impl RemoteDevice for FakeDevice {
        fn name(&self) -> &str {
            self.device_name
        }

        fn capabilities(&self) -> &[CapabilityDefinition] {
            &self.capabilities
        }

        fn invoke(&mut self, capability: &str, _arguments: &str) -> Outcome {
            Outcome::Ok {
                text: format!("handled by {}::{capability}", self.device_name),
                mutated: false,
            }
        }
    }

    fn dispatcher_with_native_ping()
    -> ToolDispatcher<TestComputer, NoWallClock, NoHttpFetcher, NoMcpClient, FakeDevice> {
        let mut dispatcher = ToolDispatcher::new();
        dispatcher.register(Tools::Ping(PingTool::new()));
        dispatcher
    }

    fn call(name: &str) -> ToolCall {
        ToolCall {
            name: name.to_string(),
            arguments: "{}".to_string(),
        }
    }

    #[test]
    fn a_bare_name_reaches_the_one_device_that_publishes_it() {
        let mut dispatcher = dispatcher_with_native_ping();
        dispatcher.register(Tools::Devices(vec![DeviceToolset::new(
            FakeDevice::publishing("pc", "ping"),
        )]));

        let outcome = dispatcher.dispatch(call("ping")).unwrap();

        assert_eq!(outcome.text, "handled by pc::ping");
    }

    #[test]
    fn a_prefixed_name_still_reaches_the_device() {
        let mut dispatcher = dispatcher_with_native_ping();
        dispatcher.register(Tools::Devices(vec![DeviceToolset::new(
            FakeDevice::publishing("pc", "ping"),
        )]));

        let outcome = dispatcher.dispatch(call("pc_ping")).unwrap();

        assert_eq!(outcome.text, "handled by pc::ping");
    }

    #[test]
    fn a_bare_name_falls_back_to_the_native_tool_when_no_device_publishes_it() {
        let mut dispatcher = dispatcher_with_native_ping();

        let outcome = dispatcher.dispatch(call("ping")).unwrap();

        assert_ne!(outcome.text, "handled by pc::ping");
    }

    #[test]
    fn an_ambiguous_bare_name_falls_back_to_the_native_tool() {
        let mut dispatcher = dispatcher_with_native_ping();
        dispatcher.register(Tools::Devices(vec![
            DeviceToolset::new(FakeDevice::publishing("pc", "ping")),
            DeviceToolset::new(FakeDevice::publishing("laptop", "ping")),
        ]));

        let outcome = dispatcher.dispatch(call("ping")).unwrap();

        assert_ne!(outcome.text, "handled by pc::ping");
        assert_ne!(outcome.text, "handled by laptop::ping");
    }

    #[test]
    fn a_device_registered_after_the_dispatcher_was_built_is_picked_up() {
        let registry = Arc::new(DeviceRegistry::new());
        let mut dispatcher = dispatcher_with_native_ping().with_device_registry(registry.clone());

        // No device connected yet: the bare name falls back to the native.
        let outcome = dispatcher.dispatch(call("ping")).unwrap();
        assert_ne!(outcome.text, "handled by pc::ping");

        // The daemon connects mid-session, after the dispatcher was built.
        registry.register("pc".to_string(), FakeDevice::publishing("pc", "ping"));

        let outcome = dispatcher.dispatch(call("ping")).unwrap();
        assert_eq!(outcome.text, "handled by pc::ping");
    }

    #[test]
    fn a_device_removed_from_the_registry_is_dropped() {
        let registry = Arc::new(DeviceRegistry::new());
        registry.register("pc".to_string(), FakeDevice::publishing("pc", "ping"));
        let mut dispatcher = dispatcher_with_native_ping().with_device_registry(registry.clone());

        registry.remove("pc");

        let outcome = dispatcher.dispatch(call("ping")).unwrap();
        assert_ne!(outcome.text, "handled by pc::ping");
    }

    #[test]
    fn device_tools_reflects_currently_registered_devices() {
        let mut dispatcher = dispatcher_with_native_ping();
        assert!(dispatcher.device_tools().is_empty());

        dispatcher.register(Tools::Devices(vec![DeviceToolset::new(
            FakeDevice::publishing("pc", "ping"),
        )]));

        let names: Vec<String> = dispatcher
            .device_tools()
            .into_iter()
            .map(|definition| definition.name)
            .collect();

        assert_eq!(names, vec!["pc_ping".to_string()]);
    }
}
