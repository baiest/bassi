use crate::fake_computer::FakeComputer;
use nala::application::tools::Tool;
use nala::application::tools::execute_command::{ExecuteCommandArgs, ExecuteCommandTool};

#[test]
fn executes_command() {
    let computer = FakeComputer::new();
    let mut tool: ExecuteCommandTool<FakeComputer> = ExecuteCommandTool::new(computer);

    let args = ExecuteCommandArgs {
        command: "start chrome".to_string(),
    };

    let result = tool.execute(args);

    assert!(result.is_ok());
    assert_eq!(
        tool.computer.executed_command,
        Some("start chrome".to_string())
    )
}

#[test]
fn returns_error_when_computer_fails() {
    let mut computer = FakeComputer::new();
    computer.should_fail = true;

    let mut tool = ExecuteCommandTool::new(computer);
    let args = ExecuteCommandArgs {
        command: "start chrome".to_string(),
    };

    let result = tool.execute(args);

    assert!(result.is_err());
}
