use chrono::{DateTime, Local};
use device_capabilities::Capability;
use device_capabilities::capabilities::execute_command::ExecuteCommandTool;
use device_capabilities::capabilities::list_apps::ListAppsTool;
use device_capabilities::capabilities::open_app::OpenAppTool;
use device_capabilities::capabilities::open_url::OpenUrlTool;
use device_capabilities::capabilities::volume::VolumeTool;
use device_capabilities::ports::computer::Computer;
use mcp::McpClient;

use crate::{
    application::tools::{
        Tool, current_time::CurrentTimeTool, fetch_url::FetchUrlTool, get_weather::GetWeatherTool,
        mcp_toolset::McpToolset, ping::PingTool, web_search::WebSearchTool,
    },
    ports::{
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

/// One variant per `Tool` implementation the dispatcher knows how to run.
/// Adding a native tool means adding a variant here and a match arm below —
/// both checked exhaustively at compile time, no runtime type erasure. MCP
/// tools all share one variant, since their identities are only known once
/// connected to a server.
pub enum Tools<
    C: Computer,
    CL: WallClock = NoWallClock,
    H: HttpFetcher = NoHttpFetcher,
    M: McpClient = NoMcpClient,
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
    // One `McpToolset` per connected server, so a tool-name collision
    // across servers is resolved by taking the first one that handles it —
    // see `dispatch` below.
    Mcp(Vec<McpToolset<M>>),
}

pub struct ToolDispatcher<
    C: Computer,
    CL: WallClock = NoWallClock,
    H: HttpFetcher = NoHttpFetcher,
    M: McpClient = NoMcpClient,
> {
    tools: Vec<Tools<C, CL, H, M>>,
}

impl<C: Computer, CL: WallClock, H: HttpFetcher, M: McpClient> ToolDispatcher<C, CL, H, M> {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    pub fn register(&mut self, tool: Tools<C, CL, H, M>) {
        self.tools.push(tool);
    }
}

impl<C: Computer, CL: WallClock, H: HttpFetcher, M: McpClient> Default
    for ToolDispatcher<C, CL, H, M>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<C: Computer, CL: WallClock, H: HttpFetcher, M: McpClient> ToolDispatcherPort
    for ToolDispatcher<C, CL, H, M>
{
    type Output = ToolOutcome;
    type Error = ToolDispatcherError;

    fn dispatch(&mut self, tool_call: ToolCall) -> Result<Self::Output, Self::Error> {
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
