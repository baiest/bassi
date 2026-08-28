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
fn returns_explicit_success_when_command_succeeds_with_no_output() {
    let computer = FakeComputer::new();
    let mut tool: ExecuteCommandTool<FakeComputer> = ExecuteCommandTool::new(computer);

    let args = ExecuteCommandArgs {
        command: "mkdir Nala1".to_string(),
    };

    let result = tool.execute(args).expect("expected success");

    assert!(result.starts_with("SUCCESS"));
}

#[test]
fn returns_explicit_success_when_command_succeeds_with_output() {
    let mut computer = FakeComputer::new();
    computer.output = "some stdout".to_string();
    let mut tool: ExecuteCommandTool<FakeComputer> = ExecuteCommandTool::new(computer);

    let args = ExecuteCommandArgs {
        command: "dir".to_string(),
    };

    let result = tool.execute(args).expect("expected success");

    assert!(result.starts_with("SUCCESS"));
    assert!(result.contains("some stdout"));
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
