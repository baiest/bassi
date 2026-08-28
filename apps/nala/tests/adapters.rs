#[path = "common/fake_process.rs"]
mod fake_process;

#[path = "common/fake_environment.rs"]
mod fake_environment;

#[path = "common/http_stub.rs"]
mod http_stub;

#[path = "common/fake_transport.rs"]
mod fake_transport;

#[path = "adapters/computer/windows.rs"]
mod computer;

#[path = "adapters/llm/ollama.rs"]
mod ollama;

#[path = "adapters/events/console.rs"]
mod console;

#[path = "adapters/mcp/stdio.rs"]
mod mcp_stdio;
