use crate::fake_computer::FakeComputer;
use nala::application::tools::Tool;
use nala::application::tools::dispatcher::{ToolDispatcher, ToolDispatcherError};
use nala::application::tools::execute_command::{ExecuteCommandArgs, ExecuteCommandTool};
use nala::ports::llm::ToolCall;
use nala::ports::tool_dispatcher::ToolDispatcher as _;

#[test]
fn executes_requested_tool() {
    let computer = FakeComputer::new();
    let tool = ExecuteCommandTool::new(computer);

    let mut dispatcher = ToolDispatcher::new();

    dispatcher.register(tool);

    let args = ExecuteCommandArgs {
        command: "start chrome".to_string(),
    };

    let result = dispatcher.execute("execute_command", args);

    assert!(result.is_ok());
}

#[test]
fn returns_error_when_tool_is_not_found() {
    let computer = FakeComputer::new();
    let tool = ExecuteCommandTool::new(computer);

    let mut dispatcher = ToolDispatcher::new();

    dispatcher.register(tool);

    let args = ExecuteCommandArgs {
        command: "start chrome".to_string(),
    };

    let result = dispatcher.execute("unknown_tool", args);

    assert!(matches!(result, Err(ToolDispatcherError::ToolNotFound)));
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
fn dispatches_a_tool_call() {
    let computer = FakeComputer::new();
    let tool = ExecuteCommandTool::new(computer);

    let mut dispatcher = ToolDispatcher::new();

    dispatcher.register(tool);

    let tool_call = ToolCall {
        name: "execute_command".to_string(),
        arguments: r#"{"command":"start chrome"}"#.to_string(),
    };

    let result = dispatcher.dispatch(tool_call);

    assert!(result.is_ok())
}
