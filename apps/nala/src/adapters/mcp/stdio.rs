use serde::Deserialize;
use serde_json::{Value, json};

use crate::ports::mcp::{McpClient, McpError, McpToolInfo, McpToolResult};

/// The line-based transport a `StdioMcpClient` speaks JSON-RPC over. Kept as
/// a trait so the client can be tested against an in-memory fake instead of
/// a real child process.
pub trait Transport {
    fn send_line(&mut self, line: &str) -> std::io::Result<()>;
    fn read_line(&mut self) -> std::io::Result<String>;
}

pub struct StdioMcpClient<T: Transport> {
    transport: T,
    next_id: u64,
}

impl<T: Transport> StdioMcpClient<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            next_id: 1,
        }
    }

    /// Exposed for tests, to inspect what was sent over the transport.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, McpError> {
        let id = self.next_id;
        self.next_id += 1;

        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let line = serde_json::to_string(&request)
            .map_err(|error| McpError::Protocol(error.to_string()))?;

        self.transport
            .send_line(&line)
            .map_err(|error| McpError::Transport(error.to_string()))?;

        // The server can interleave asynchronous notifications (no "id",
        // e.g. a resource-updated notice after a screenshot) or responses
        // to other in-flight requests between our request and its actual
        // response. Keep reading until a message carries this request's id.
        loop {
            let response_line = self
                .transport
                .read_line()
                .map_err(|error| McpError::Transport(error.to_string()))?;

            let response: JsonRpcResponse = serde_json::from_str(&response_line)
                .map_err(|error| McpError::Protocol(error.to_string()))?;

            if response.id != Some(id) {
                continue;
            }

            if let Some(error) = response.error {
                return Err(McpError::Protocol(error.message));
            }

            return response
                .result
                .ok_or_else(|| McpError::Protocol("response had no result".to_string()));
        }
    }
}

impl<T: Transport> McpClient for StdioMcpClient<T> {
    fn list_tools(&mut self) -> Result<Vec<McpToolInfo>, McpError> {
        let result = self.request("tools/list", json!({}))?;

        let response: ToolsListResult = serde_json::from_value(result)
            .map_err(|error| McpError::Protocol(error.to_string()))?;

        Ok(response
            .tools
            .into_iter()
            .map(|tool| McpToolInfo {
                name: tool.name,
                description: tool.description.unwrap_or_default(),
                parameters: tool
                    .input_schema
                    .unwrap_or_else(|| json!({"type": "object"})),
            })
            .collect())
    }

    fn call_tool(&mut self, name: &str, arguments: Value) -> Result<McpToolResult, McpError> {
        let result = self.request(
            "tools/call",
            json!({
                "name": name,
                "arguments": arguments,
            }),
        )?;

        let response: CallToolResult = serde_json::from_value(result)
            .map_err(|error| McpError::Protocol(error.to_string()))?;

        let mut text_parts = Vec::new();
        let mut images = Vec::new();

        for item in response.content {
            match item {
                ContentItem::Text { text } => text_parts.push(text),
                ContentItem::Image { data, .. } => images.push(data),
                ContentItem::Other => {}
            }
        }

        let text = text_parts.join("\n");

        if response.is_error {
            return Err(McpError::ToolFailed(text));
        }

        Ok(McpToolResult { text, images })
    }
}

#[derive(Deserialize)]
struct JsonRpcResponse {
    #[serde(default)]
    id: Option<u64>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<JsonRpcError>,
}

#[derive(Deserialize)]
struct JsonRpcError {
    message: String,
}

#[derive(Deserialize)]
struct ToolsListResult {
    tools: Vec<ToolInfoWire>,
}

#[derive(Deserialize)]
struct ToolInfoWire {
    name: String,
    description: Option<String>,
    #[serde(rename = "inputSchema")]
    input_schema: Option<Value>,
}

#[derive(Deserialize)]
struct CallToolResult {
    #[serde(default)]
    content: Vec<ContentItem>,
    #[serde(default, rename = "isError")]
    is_error: bool,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum ContentItem {
    Text {
        text: String,
    },
    Image {
        data: String,
        #[allow(dead_code)]
        mime_type: Option<String>,
    },
    #[serde(other)]
    Other,
}
