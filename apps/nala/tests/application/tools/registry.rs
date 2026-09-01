use crate::fake_computer::FakeComputer;
use device_capabilities::Capability;
use device_capabilities::capabilities::execute_command::ExecuteCommandTool;
use nala::application::tools::ToolDefinition;
use nala::application::tools::registry::ToolRegistry;

fn execute_command_definition() -> ToolDefinition {
    ExecuteCommandTool::<FakeComputer>::definition().into()
}

#[test]
fn can_register_and_find_a_tool() {
    let mut registry = ToolRegistry::new();

    registry.register(execute_command_definition());

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

    let execute_command = execute_command_definition();

    let another_tool = ToolDefinition {
        name: "another_tool".to_string(),
        description: "another".to_string(),
        parameters: serde_json::json!({}),
    };

    registry.register(execute_command);
    registry.register(another_tool);

    assert_eq!(
        registry.get("execute_command").unwrap().name,
        "execute_command"
    );

    assert_eq!(registry.get("another_tool").unwrap().name, "another_tool");
}

#[test]
fn can_register_a_tool_with_a_runtime_discovered_name() {
    let mut registry = ToolRegistry::new();

    // MCP tools are discovered via `tools/list` at runtime, so their name
    // is a `String`, not a `&'static str` known at compile time.
    let discovered_name = String::from("screenshot");

    registry.register(ToolDefinition {
        name: discovered_name.clone(),
        description: "Take a screenshot".to_string(),
        parameters: serde_json::json!({}),
    });

    assert_eq!(registry.get(&discovered_name).unwrap().name, "screenshot");
}
