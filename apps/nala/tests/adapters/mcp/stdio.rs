use nala::adapters::mcp::stdio::StdioMcpClient;
use nala::ports::mcp::{McpClient, McpError};
use serde_json::json;

use crate::fake_transport::FakeTransport;

#[test]
fn list_tools_sends_the_expected_json_rpc_request() {
    let transport =
        FakeTransport::with_responses(vec![r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#]);
    let mut client = StdioMcpClient::new(transport);

    client.list_tools().unwrap();

    let sent: serde_json::Value = serde_json::from_str(&client.transport().sent[0]).unwrap();
    assert_eq!(sent["method"], "tools/list");
    assert_eq!(sent["jsonrpc"], "2.0");
}

#[test]
fn list_tools_parses_returned_tools() {
    let transport = FakeTransport::with_responses(vec![
        r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[
            {"name":"screenshot","description":"Take a screenshot","inputSchema":{"type":"object"}}
        ]}}"#,
    ]);
    let mut client = StdioMcpClient::new(transport);

    let tools = client.list_tools().unwrap();

    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "screenshot");
    assert_eq!(tools[0].description, "Take a screenshot");
}

#[test]
fn call_tool_sends_name_and_arguments() {
    let transport = FakeTransport::with_responses(vec![
        r#"{"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"ok"}]}}"#,
    ]);
    let mut client = StdioMcpClient::new(transport);

    client
        .call_tool("left_click", json!({"x": 10, "y": 20}))
        .unwrap();

    let sent: serde_json::Value = serde_json::from_str(&client.transport().sent[0]).unwrap();
    assert_eq!(sent["method"], "tools/call");
    assert_eq!(sent["params"]["name"], "left_click");
    assert_eq!(sent["params"]["arguments"]["x"], 10);
}

#[test]
fn call_tool_collects_text_and_image_content() {
    let transport = FakeTransport::with_responses(vec![
        r#"{"jsonrpc":"2.0","id":1,"result":{"content":[
            {"type":"text","text":"here is the screen"},
            {"type":"image","data":"YmFzZTY0ZGF0YQ==","mimeType":"image/png"}
        ]}}"#,
    ]);
    let mut client = StdioMcpClient::new(transport);

    let result = client.call_tool("screenshot", json!({})).unwrap();

    assert_eq!(result.text, "here is the screen");
    assert_eq!(result.images, vec!["YmFzZTY0ZGF0YQ==".to_string()]);
}

#[test]
fn call_tool_reports_tool_side_errors() {
    let transport = FakeTransport::with_responses(vec![
        r#"{"jsonrpc":"2.0","id":1,"result":{"isError":true,"content":[{"type":"text","text":"click target not found"}]}}"#,
    ]);
    let mut client = StdioMcpClient::new(transport);

    let result = client.call_tool("left_click", json!({}));

    assert!(
        matches!(result, Err(McpError::ToolFailed(message)) if message == "click target not found")
    );
}

#[test]
fn a_non_json_response_line_is_a_protocol_error() {
    let transport = FakeTransport::with_responses(vec!["not json at all"]);
    let mut client = StdioMcpClient::new(transport);

    let result = client.list_tools();

    assert!(matches!(result, Err(McpError::Protocol(_))));
}

#[test]
fn skips_notifications_interleaved_before_the_matching_response() {
    // MCP servers can emit async notifications (no "id") in between a
    // request and its response — e.g. computer-use-mcp announcing a
    // resource update after taking a screenshot. A notification must not
    // be mistaken for the response.
    let transport = FakeTransport::with_responses(vec![
        r#"{"jsonrpc":"2.0","method":"notifications/resources/updated","params":{}}"#,
        r#"{"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"ok"}]}}"#,
    ]);
    let mut client = StdioMcpClient::new(transport);

    let result = client.call_tool("screenshot", json!({}));

    assert_eq!(result.unwrap().text, "ok");
}

#[test]
fn ignores_a_response_whose_id_does_not_match_the_request() {
    let transport = FakeTransport::with_responses(vec![
        r#"{"jsonrpc":"2.0","id":999,"result":{"tools":[]}}"#,
        r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#,
    ]);
    let mut client = StdioMcpClient::new(transport);

    let result = client.list_tools();

    assert!(result.is_ok());
}

#[test]
fn a_transport_read_failure_is_a_transport_error() {
    let mut transport = FakeTransport::new();
    transport.fail_read = true;
    let mut client = StdioMcpClient::new(transport);

    let result = client.list_tools();

    assert!(matches!(result, Err(McpError::Transport(_))));
}
