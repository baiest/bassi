use crate::fake_computer::FakeComputer;
use nala::application::tools::execute_command::ExecuteCommandTool;
use nala::application::tools::registry::ToolRegistry;
use nala::application::tools::{Tool, ToolDefinition};

#[test]
fn can_register_and_find_a_tool() {
    let mut registry = ToolRegistry::new();

    registry.register(ExecuteCommandTool::<FakeComputer>::definition());

    let definition = registry.get("execute_command");

    assert!(definition.is_some());
    assert_eq!(definition.unwrap().name, "execute_command");
}

#[test]
fn returns_none_when_tool_does_not_exist() {
    let registry = ToolRegistry::new();

    assert!(registry.get("unknown").is_none());
}

#[test]
fn can_register_multiple_tools() {
    let mut registry = ToolRegistry::new();

    let execute_command = ExecuteCommandTool::<FakeComputer>::definition();

    let another_tool = ToolDefinition {
        name: "another_tool",
        description: "another",
    };

    registry.register(execute_command);
    registry.register(another_tool);

    assert_eq!(
        registry.get("execute_command").unwrap().name,
        "execute_command"
    );

    assert_eq!(registry.get("another_tool").unwrap().name, "another_tool");
}
