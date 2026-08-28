use nala::application::tools::computer_use::ComputerUseToolset;
use nala::ports::mcp::McpToolResult;

use crate::fake_mcp::FakeMcpClient;

#[test]
fn connect_only_exposes_allowlisted_tools() {
    let client = FakeMcpClient::new()
        .with_tool("screenshot", "Take a screenshot")
        .with_tool("left_click", "Click the mouse")
        .with_tool("run_script", "Run an arbitrary script");

    let toolset = ComputerUseToolset::connect(client, &["screenshot", "left_click"]).unwrap();

    let names: Vec<&str> = toolset
        .definitions()
        .iter()
        .map(|definition| definition.name.as_str())
        .collect();

    assert_eq!(names.len(), 2);
    assert!(names.contains(&"screenshot"));
    assert!(names.contains(&"left_click"));
    assert!(!names.contains(&"run_script"));
}

#[test]
fn handles_reports_whether_a_tool_name_is_registered() {
    let client = FakeMcpClient::new().with_tool("screenshot", "Take a screenshot");

    let toolset = ComputerUseToolset::connect(client, &["screenshot"]).unwrap();

    assert!(toolset.handles("screenshot"));
    assert!(!toolset.handles("left_click"));
}

#[test]
fn call_forwards_name_and_parsed_arguments_to_the_mcp_client() {
    let client = FakeMcpClient::new()
        .with_tool("left_click", "Click the mouse")
        .returning(McpToolResult {
            text: "clicked".to_string(),
            images: vec![],
        });

    let mut toolset = ComputerUseToolset::connect(client, &["left_click"]).unwrap();

    let result = toolset.call("left_click", r#"{"x":10,"y":20}"#).unwrap();

    assert_eq!(result.text, "clicked");
    assert_eq!(
        toolset.client().calls[0],
        (
            "left_click".to_string(),
            serde_json::json!({"x": 10, "y": 20})
        )
    );
}

#[test]
fn call_returns_the_underlying_error_when_the_mcp_client_fails() {
    let client = FakeMcpClient::new()
        .with_tool("left_click", "Click the mouse")
        .failing_calls_with("click target not found");

    let mut toolset = ComputerUseToolset::connect(client, &["left_click"]).unwrap();

    let result = toolset.call("left_click", "{}");

    assert!(result.is_err());
}
