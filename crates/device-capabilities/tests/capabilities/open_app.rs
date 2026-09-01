use crate::fake_computer::FakeComputer;
use device_capabilities::Capability;
use device_capabilities::capabilities::open_app::{OpenAppArgs, OpenAppTool};
use schemars::schema_for;

#[test]
fn opens_an_app_by_name() {
    let computer = FakeComputer::new();
    let mut tool: OpenAppTool<FakeComputer> = OpenAppTool::new(computer);

    let args = OpenAppArgs {
        app: "notepad".to_string(),
    };

    let result = tool.execute(args);

    assert!(result.is_ok());
    assert_eq!(
        tool.computer.executed_command,
        Some("start \"\" \"notepad\"".to_string())
    );
}

#[test]
fn opens_an_app_by_full_path() {
    let computer = FakeComputer::new();
    let mut tool: OpenAppTool<FakeComputer> = OpenAppTool::new(computer);

    let args = OpenAppArgs {
        app: r"C:\Windows\System32\calc.exe".to_string(),
    };

    let result = tool.execute(args);

    assert!(result.is_ok());
}

#[test]
fn rejects_an_empty_app_without_touching_the_computer() {
    let computer = FakeComputer::new();
    let mut tool: OpenAppTool<FakeComputer> = OpenAppTool::new(computer);

    let args = OpenAppArgs {
        app: "   ".to_string(),
    };

    let result = tool.execute(args);

    assert!(result.is_err());
    assert_eq!(tool.computer.executed_command, None);
}

#[test]
fn rejects_an_app_containing_a_quote_without_touching_the_computer() {
    let computer = FakeComputer::new();
    let mut tool: OpenAppTool<FakeComputer> = OpenAppTool::new(computer);

    let args = OpenAppArgs {
        app: "notepad\" & calc.exe".to_string(),
    };

    let result = tool.execute(args);

    assert!(result.is_err());
    assert_eq!(tool.computer.executed_command, None);
}

#[test]
fn returns_error_when_computer_fails() {
    let mut computer = FakeComputer::new();
    computer.should_fail = true;
    let mut tool = OpenAppTool::new(computer);

    let args = OpenAppArgs {
        app: "notepad".to_string(),
    };

    let result = tool.execute(args);

    assert!(result.is_err());
}

#[test]
fn published_schema_matches_the_derived_schema_of_args() {
    let expected =
        serde_json::to_value(schema_for!(OpenAppArgs)).expect("schema should serialize to JSON");

    let definition = OpenAppTool::<FakeComputer>::definition();

    assert_eq!(definition.parameters, expected);
}
