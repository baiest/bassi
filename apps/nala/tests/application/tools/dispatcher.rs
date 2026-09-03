use crate::fake_computer::FakeComputer;
use crate::fake_device::FakeDevice;
use crate::fake_mcp::FakeMcpClient;
use device_capabilities::Capability;
use device_capabilities::capabilities::execute_command::ExecuteCommandTool;
use device_capabilities::capabilities::list_apps::ListAppsTool;
use device_capabilities::capabilities::open_app::OpenAppTool;
use device_capabilities::capabilities::open_url::OpenUrlTool;
use device_capabilities::capabilities::volume::VolumeTool;
use mcp::McpToolResult;
use nala::adapters::memory::in_memory::InMemoryMemoryStore;
use nala::application::tools::device_toolset::DeviceToolset;
use nala::application::tools::dispatcher::{
    NoHttpFetcher, NoMcpClient, NoWallClock, ToolDispatcher, ToolDispatcherError, Tools,
};
use nala::application::tools::mcp_toolset::McpToolset;
use nala::application::tools::ping::PingTool;
use nala::application::tools::remember::RememberTool;
use nala::ports::llm::ToolCall;
use nala::ports::tool_dispatcher::ToolDispatcher as _;

#[test]
fn executes_requested_tool() {
    let computer = FakeComputer::new();
    let tool = ExecuteCommandTool::new(computer);

    let mut dispatcher = ToolDispatcher::<FakeComputer>::new();

    dispatcher.register(Tools::ExecuteCommand(tool));

    let tool_call = ToolCall {
        name: "execute_command".to_string(),
        arguments: r#"{"command":"start chrome"}"#.to_string(),
    };

    let result = dispatcher.dispatch(tool_call);

    assert!(result.is_ok());
}

#[test]
fn returns_error_when_tool_is_not_found() {
    let computer = FakeComputer::new();
    let tool = ExecuteCommandTool::new(computer);

    let mut dispatcher = ToolDispatcher::<FakeComputer>::new();

    dispatcher.register(Tools::ExecuteCommand(tool));

    let tool_call = ToolCall {
        name: "unknown_tool".to_string(),
        arguments: r#"{"command":"start chrome"}"#.to_string(),
    };

    let result = dispatcher.dispatch(tool_call);

    assert!(matches!(result, Err(ToolDispatcherError::ToolNotFound)));
}

#[test]
fn dispatches_to_the_matching_tool_among_several() {
    let computer = FakeComputer::new();
    let execute_command = ExecuteCommandTool::new(computer);

    let mut dispatcher = ToolDispatcher::<FakeComputer>::new();

    dispatcher.register(Tools::ExecuteCommand(execute_command));
    dispatcher.register(Tools::Ping(PingTool::new()));

    let ping_call = ToolCall {
        name: "ping".to_string(),
        arguments: "{}".to_string(),
    };

    let execute_call = ToolCall {
        name: "execute_command".to_string(),
        arguments: r#"{"command":"start chrome"}"#.to_string(),
    };

    assert_eq!(dispatcher.dispatch(ping_call).unwrap().text, "pong");
    assert!(
        dispatcher
            .dispatch(execute_call)
            .unwrap()
            .text
            .starts_with("Command executed")
    );
}

#[test]
fn returns_computer_context_from_the_registered_computer_tool() {
    let computer = FakeComputer::new();
    let tool = ExecuteCommandTool::new(computer);

    let mut dispatcher = ToolDispatcher::<FakeComputer>::new();
    dispatcher.register(Tools::ExecuteCommand(tool));

    let result = dispatcher.get_context();

    assert!(result.is_ok());
}

#[test]
fn returns_error_getting_context_without_a_computer_tool() {
    let mut dispatcher: ToolDispatcher<FakeComputer> = ToolDispatcher::new();
    dispatcher.register(Tools::Ping(PingTool::new()));

    let result = dispatcher.get_context();

    assert!(matches!(result, Err(ToolDispatcherError::ToolNotFound)));
}

#[test]
fn returns_error_when_dispatching_with_invalid_arguments() {
    let computer = FakeComputer::new();
    let tool = ExecuteCommandTool::new(computer);

    let mut dispatcher = ToolDispatcher::<FakeComputer>::new();
    dispatcher.register(Tools::ExecuteCommand(tool));

    let tool_call = ToolCall {
        name: "execute_command".to_string(),
        arguments: "not json".to_string(),
    };

    let result = dispatcher.dispatch(tool_call);

    assert!(matches!(
        result,
        Err(ToolDispatcherError::ToolErrorParsingArguments(_))
    ));
}

#[test]
fn parses_tool_call_arguments() {
    let tool_call = ToolCall {
        name: "execute_command".to_string(),
        arguments: r#"{"command":"start chrome"}"#.to_string(),
    };

    let args = ExecuteCommandTool::<FakeComputer>::parse_arguments(&tool_call.arguments)
        .expect("Failed to parse arguments");

    assert_eq!(args.command, "start chrome")
}

#[test]
fn marks_execute_command_outcome_as_mutated() {
    let computer = FakeComputer::new();
    let tool = ExecuteCommandTool::new(computer);

    let mut dispatcher = ToolDispatcher::<FakeComputer>::new();
    dispatcher.register(Tools::ExecuteCommand(tool));

    let tool_call = ToolCall {
        name: "execute_command".to_string(),
        arguments: r#"{"command":"start chrome"}"#.to_string(),
    };

    let outcome = dispatcher.dispatch(tool_call).unwrap();

    assert!(outcome.mutated);
}

#[test]
fn does_not_mark_ping_outcome_as_mutated() {
    let mut dispatcher = ToolDispatcher::<FakeComputer>::new();
    dispatcher.register(Tools::Ping(PingTool::new()));

    let tool_call = ToolCall {
        name: "ping".to_string(),
        arguments: "{}".to_string(),
    };

    let outcome = dispatcher.dispatch(tool_call).unwrap();

    assert!(!outcome.mutated);
}

#[test]
fn routes_a_remember_call_to_the_remember_tool_and_marks_it_mutated() {
    let mut dispatcher = ToolDispatcher::<FakeComputer>::new();
    dispatcher.register(Tools::Remember(RememberTool::new(Box::new(
        InMemoryMemoryStore::new(),
    ))));

    let tool_call = ToolCall {
        name: "remember".to_string(),
        arguments: r#"{"key":"nombre","value":"Juan"}"#.to_string(),
    };

    let outcome = dispatcher.dispatch(tool_call).unwrap();

    assert!(outcome.mutated);
    assert!(outcome.text.contains("nombre"));
}

#[test]
fn attaches_before_and_after_computer_state_to_execute_command() {
    let computer = FakeComputer::new();
    let tool = ExecuteCommandTool::new(computer);

    let mut dispatcher = ToolDispatcher::<FakeComputer>::new();
    dispatcher.register(Tools::ExecuteCommand(tool));

    let tool_call = ToolCall {
        name: "execute_command".to_string(),
        arguments: r#"{"command":"start chrome"}"#.to_string(),
    };

    let outcome = dispatcher.dispatch(tool_call).unwrap();

    // FakeComputer's context doesn't change between the two snapshots, so
    // this exercises the "unchanged" branch rather than a diff — the
    // "changed" branch is exercised end-to-end by the real Windows adapter.
    assert!(outcome.text.contains("State unchanged:"));
}

#[test]
fn dispatches_open_url_and_marks_it_mutated() {
    let computer = FakeComputer::new();
    let mut dispatcher = ToolDispatcher::<FakeComputer>::new();
    dispatcher.register(Tools::OpenUrl(OpenUrlTool::new(computer)));

    let tool_call = ToolCall {
        name: "open_url".to_string(),
        arguments: r#"{"url":"https://example.com"}"#.to_string(),
    };

    let outcome = dispatcher.dispatch(tool_call).unwrap();

    assert!(outcome.mutated);
}

#[test]
fn dispatches_open_app_and_marks_it_mutated() {
    let computer = FakeComputer::new();
    let mut dispatcher = ToolDispatcher::<FakeComputer>::new();
    dispatcher.register(Tools::OpenApp(OpenAppTool::new(computer)));

    let tool_call = ToolCall {
        name: "open_app".to_string(),
        arguments: r#"{"app":"notepad"}"#.to_string(),
    };

    let outcome = dispatcher.dispatch(tool_call).unwrap();

    assert!(outcome.mutated);
}

#[test]
fn dispatches_volume_and_marks_it_mutated() {
    let computer = FakeComputer::new();
    let mut dispatcher = ToolDispatcher::<FakeComputer>::new();
    dispatcher.register(Tools::Volume(VolumeTool::new(computer)));

    let tool_call = ToolCall {
        name: "volume".to_string(),
        arguments: r#"{"action":"up"}"#.to_string(),
    };

    let outcome = dispatcher.dispatch(tool_call).unwrap();

    assert!(outcome.mutated);
}

#[test]
fn dispatches_list_apps_and_does_not_mark_it_mutated() {
    let mut computer = FakeComputer::new();
    computer.output = r#"[{"Name":"Notepad","AppID":"notepad.exe"}]"#.to_string();
    let mut dispatcher = ToolDispatcher::<FakeComputer>::new();
    dispatcher.register(Tools::ListApps(ListAppsTool::new(computer)));

    let tool_call = ToolCall {
        name: "list_apps".to_string(),
        arguments: "{}".to_string(),
    };

    let outcome = dispatcher.dispatch(tool_call).unwrap();

    assert!(outcome.text.contains("Notepad"));
    assert!(!outcome.mutated);
}

#[test]
fn never_marks_mcp_tool_calls_as_mutated() {
    // The MCP protocol has no way to say whether a call changed anything
    // server-side, so the dispatcher never sets `mutated` for it — even
    // for a tool whose name suggests it acts, like "send_message".
    let client = FakeMcpClient::new()
        .with_tool("send_message", "Send a chat message")
        .returning(McpToolResult {
            text: "sent".to_string(),
            images: vec![],
        });
    let toolset = McpToolset::connect(client, Some(&["send_message"])).unwrap();

    let mut dispatcher =
        ToolDispatcher::<FakeComputer, NoWallClock, NoHttpFetcher, FakeMcpClient>::new();
    dispatcher.register(Tools::Mcp(vec![toolset]));

    let tool_call = ToolCall {
        name: "send_message".to_string(),
        arguments: "{}".to_string(),
    };

    let outcome = dispatcher.dispatch(tool_call).unwrap();

    assert!(!outcome.mutated);
}

#[test]
fn dispatches_to_the_mcp_toolset_and_carries_images_through() {
    let client = FakeMcpClient::new()
        .with_tool("search", "Search the web")
        .returning(McpToolResult {
            text: "here are the results".to_string(),
            images: vec!["YmFzZTY0ZGF0YQ==".to_string()],
        });
    let toolset = McpToolset::connect(client, Some(&["search"])).unwrap();

    let mut dispatcher =
        ToolDispatcher::<FakeComputer, NoWallClock, NoHttpFetcher, FakeMcpClient>::new();
    dispatcher.register(Tools::Mcp(vec![toolset]));

    let tool_call = ToolCall {
        name: "search".to_string(),
        arguments: "{}".to_string(),
    };

    let outcome = dispatcher.dispatch(tool_call).unwrap();

    assert_eq!(outcome.text, "here are the results");
    assert_eq!(outcome.images, vec!["YmFzZTY0ZGF0YQ==".to_string()]);
}

#[test]
fn the_dispatcher_routes_a_prefixed_tool_call_to_its_device() {
    let device = FakeDevice::new("pc")
        .with_capability("open_app", "Opens an app")
        .returning(device_protocol::Outcome::Ok {
            text: "opened Spotify".to_string(),
            mutated: true,
        });
    let toolset = DeviceToolset::new(device);

    let mut dispatcher =
        ToolDispatcher::<FakeComputer, NoWallClock, NoHttpFetcher, NoMcpClient, FakeDevice>::new();
    dispatcher.register(Tools::Devices(vec![toolset]));

    let tool_call = ToolCall {
        name: "pc_open_app".to_string(),
        arguments: r#"{"app":"Spotify"}"#.to_string(),
    };

    let outcome = dispatcher.dispatch(tool_call).unwrap();

    assert_eq!(outcome.text, "opened Spotify");
    assert!(outcome.mutated);
}

#[test]
fn a_tool_call_for_a_disconnected_device_is_a_tool_not_found_error() {
    let mut dispatcher =
        ToolDispatcher::<FakeComputer, NoWallClock, NoHttpFetcher, NoMcpClient, FakeDevice>::new();
    // No `Tools::Devices` registered at all — the PC daemon never
    // connected, or dropped before this call.
    dispatcher.register(Tools::Ping(PingTool::new()));

    let tool_call = ToolCall {
        name: "pc_open_app".to_string(),
        arguments: "{}".to_string(),
    };

    let result = dispatcher.dispatch(tool_call);

    assert!(matches!(result, Err(ToolDispatcherError::ToolNotFound)));
}
