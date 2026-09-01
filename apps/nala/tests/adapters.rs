#[path = "common/http_stub.rs"]
mod http_stub;

#[path = "common/fake_events.rs"]
mod fake_events;

#[path = "common/fake_device.rs"]
#[allow(dead_code)]
mod fake_device;

#[path = "adapters/devices/state_broadcast.rs"]
mod state_broadcast;

#[path = "adapters/llm/ollama.rs"]
mod ollama;

#[path = "adapters/events/console.rs"]
mod console;

#[path = "adapters/metrics/csv_sink.rs"]
mod csv_metrics_sink;
