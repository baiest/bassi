#[path = "common/fake_process.rs"]
mod fake_process;

#[path = "common/fake_environment.rs"]
mod fake_environment;

#[path = "common/http_stub.rs"]
mod http_stub;

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

#[path = "adapters/metrics/csv_sink.rs"]
mod csv_metrics_sink;
