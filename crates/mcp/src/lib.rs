//! Reusable MCP (Model Context Protocol) client infrastructure: the domain
//! types, the `McpClient` port, and a JSON-RPC-over-stdio implementation of
//! it. Any consumer that needs tools from an MCP server depends on this
//! crate rather than reimplementing the protocol.

mod child_process;
mod stdio;
mod types;

pub use child_process::ChildTransport;
pub use stdio::{StdioMcpClient, Transport};
pub use types::{McpClient, McpError, McpToolInfo, McpToolResult};
