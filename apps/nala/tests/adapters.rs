#[path = "common/fake_process.rs"]
mod fake_process;

#[path = "common/fake_environment.rs"]
mod fake_environment;

#[path = "common/http_stub.rs"]
mod http_stub;

#[path = "common/fake_transport.rs"]
mod fake_transport;

#[path = "common/fake_speech.rs"]
mod fake_speech;

#[path = "common/fake_tts.rs"]
mod fake_tts;

#[path = "common/fake_events.rs"]
mod fake_events;

#[path = "adapters/computer/windows.rs"]
mod computer;

// Spawns a real `cmd`/`ping` process — Windows-only, like the adapter it
// tests.
#[cfg(windows)]
#[path = "adapters/process/windows.rs"]
mod process_windows;

#[path = "adapters/llm/ollama.rs"]
mod ollama;

#[path = "adapters/events/console.rs"]
mod console;

#[path = "adapters/events/speaking.rs"]
mod speaking;

#[path = "adapters/speech/async_speech.rs"]
mod async_speech;

#[path = "adapters/speech/chatterbox_config.rs"]
mod chatterbox_config;

#[path = "adapters/speech/chatterbox.rs"]
mod chatterbox;

#[path = "adapters/speech/chatterbox_http.rs"]
mod chatterbox_http;

#[path = "adapters/audio/rodio_player.rs"]
mod rodio_player;

#[path = "adapters/speech/chatterbox_supervisor.rs"]
mod chatterbox_supervisor;

#[path = "adapters/mcp/stdio.rs"]
mod mcp_stdio;

// Spawns a real `ping` process to prove a hung MCP server can't block
// `read_line` forever — Windows-only: the deliberately-silent command it
// spawns (`ping ... >NUL`) relies on `cmd /C` redirection, which only
// `ChildTransport` sets up on Windows (see its `spawn` doc comment).
#[cfg(windows)]
#[path = "adapters/mcp/child_process.rs"]
mod mcp_child_process;
