use mcp::McpToolResult;
use nala::application::tools::mcp_toolset::McpToolset;

use crate::fake_mcp::FakeMcpClient;

#[test]
fn connect_only_exposes_allowlisted_tools_when_an_allowlist_is_given() {
    let client = FakeMcpClient::new()
        .with_tool("search", "Search the web")
        .with_tool("weather", "Get the weather")
        .with_tool("run_script", "Run an arbitrary script");

    let toolset = McpToolset::connect(client, Some(&["search", "weather"])).unwrap();

    let names: Vec<&str> = toolset
        .definitions()
        .iter()
        .map(|definition| definition.name.as_str())
        .collect();

    assert_eq!(names.len(), 2);
    assert!(names.contains(&"search"));
    assert!(names.contains(&"weather"));
    assert!(!names.contains(&"run_script"));
}

#[test]
fn connect_exposes_every_tool_when_no_allowlist_is_given() {
    let client = FakeMcpClient::new()
        .with_tool("search", "Search the web")
        .with_tool("weather", "Get the weather");

    let toolset = McpToolset::connect(client, None).unwrap();

    assert_eq!(toolset.definitions().len(), 2);
}

#[test]
fn handles_reports_whether_a_tool_name_is_registered() {
    let client = FakeMcpClient::new().with_tool("search", "Search the web");

    let toolset = McpToolset::connect(client, Some(&["search"])).unwrap();

    assert!(toolset.handles("search"));
    assert!(!toolset.handles("weather"));
}

#[test]
fn call_forwards_name_and_parsed_arguments_to_the_mcp_client() {
    let client = FakeMcpClient::new()
        .with_tool("search", "Search the web")
        .returning(McpToolResult {
            text: "results".to_string(),
            images: vec![],
        });

    let mut toolset = McpToolset::connect(client, Some(&["search"])).unwrap();

    let result = toolset.call("search", r#"{"query":"bassi"}"#).unwrap();

    assert_eq!(result.text, "results");
    assert_eq!(
        toolset.client().calls[0],
        ("search".to_string(), serde_json::json!({"query": "bassi"}))
    );
}

#[test]
fn call_returns_the_underlying_error_when_the_mcp_client_fails() {
    let client = FakeMcpClient::new()
        .with_tool("search", "Search the web")
        .failing_calls_with("no results found");

    let mut toolset = McpToolset::connect(client, Some(&["search"])).unwrap();

    let result = toolset.call("search", "{}");

    assert!(result.is_err());
}
