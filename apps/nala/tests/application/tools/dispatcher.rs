use crate::fake_computer::FakeComputer;
use nala::application::tools::Tool;
use nala::application::tools::dispatcher::{ToolDispatcher, ToolDispatcherError, Tools};
use nala::application::tools::execute_command::ExecuteCommandTool;
use nala::application::tools::ping::PingTool;
use nala::ports::llm::ToolCall;
use nala::ports::tool_dispatcher::ToolDispatcher as _;

#[test]
fn executes_requested_tool() {
    let computer = FakeComputer::new();
    let tool = ExecuteCommandTool::new(computer);

    let mut dispatcher = ToolDispatcher::new();

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

    let mut dispatcher = ToolDispatcher::new();

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

    let mut dispatcher = ToolDispatcher::new();

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

    assert_eq!(dispatcher.dispatch(ping_call).unwrap(), "pong");
    assert!(
        dispatcher
            .dispatch(execute_call)
            .unwrap()
            .starts_with("SUCCESS")
    );
}

#[test]
fn returns_computer_context_from_the_registered_computer_tool() {
    let computer = FakeComputer::new();
    let tool = ExecuteCommandTool::new(computer);

    let mut dispatcher = ToolDispatcher::new();
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

    let mut dispatcher = ToolDispatcher::new();
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
