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
fn call_attaches_a_verification_screenshot_after_a_mutating_action() {
    let client = FakeMcpClient::new()
        .with_tool("left_click", "Click the mouse")
        .with_tool("screenshot", "Take a screenshot")
        .returning(McpToolResult {
            text: "clicked".to_string(),
            images: vec!["Y2xpY2tlZA==".to_string()],
        });

    let mut toolset = ComputerUseToolset::connect(client, &["left_click", "screenshot"]).unwrap();

    let result = toolset.call("left_click", "{}").unwrap();

    // Both the click's own image (if any — here from the scripted result)
    // and the auto-verification screenshot's image end up attached.
    assert_eq!(result.images.len(), 2);
    assert!(
        result
            .text
            .contains("[Auto-verification screenshot attached")
    );
    assert_eq!(
        toolset.client().calls.len(),
        2,
        "expected the click and the auto-screenshot"
    );
    assert_eq!(toolset.client().calls[0].0, "left_click");
    assert_eq!(toolset.client().calls[1].0, "screenshot");
}

#[test]
fn call_skips_the_verification_screenshot_when_the_toolset_does_not_allow_it() {
    let client = FakeMcpClient::new()
        .with_tool("left_click", "Click the mouse")
        .returning(McpToolResult {
            text: "clicked".to_string(),
            images: vec![],
        });

    let mut toolset = ComputerUseToolset::connect(client, &["left_click"]).unwrap();

    let result = toolset.call("left_click", "{}").unwrap();

    assert_eq!(result.text, "clicked");
    assert_eq!(toolset.client().calls.len(), 1);
}

#[test]
fn call_does_not_attach_a_screenshot_after_a_read_only_action() {
    let client = FakeMcpClient::new()
        .with_tool("screenshot", "Take a screenshot")
        .returning(McpToolResult {
            text: "here is the screen".to_string(),
            images: vec!["YQ==".to_string()],
        });

    let mut toolset = ComputerUseToolset::connect(client, &["screenshot"]).unwrap();

    let result = toolset.call("screenshot", "{}").unwrap();

    assert_eq!(
        result.images.len(),
        1,
        "no extra screenshot should be attached to a screenshot call"
    );
    assert_eq!(toolset.client().calls.len(), 1);
}

#[test]
fn is_mutating_distinguishes_actions_from_reads() {
    assert!(ComputerUseToolset::<FakeMcpClient>::is_mutating(
        "left_click"
    ));
    assert!(ComputerUseToolset::<FakeMcpClient>::is_mutating("type"));
    assert!(!ComputerUseToolset::<FakeMcpClient>::is_mutating(
        "screenshot"
    ));
    assert!(!ComputerUseToolset::<FakeMcpClient>::is_mutating(
        "list_windows"
    ));
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
