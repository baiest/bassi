use crate::fake_computer::FakeComputer;
use nala::application::tools::dispatcher::{ToolDispatcher, ToolDispatcherError};
use nala::application::tools::execute_command::{ExecuteCommandArgs, ExecuteCommandTool};

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
