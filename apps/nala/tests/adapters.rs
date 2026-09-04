#[path = "common/http_stub.rs"]
mod http_stub;

#[path = "common/fake_events.rs"]
mod fake_events;

#[path = "adapters/llm/ollama.rs"]
mod ollama;

#[path = "adapters/events/console.rs"]
mod console;

#[path = "adapters/metrics/csv_sink.rs"]
mod csv_metrics_sink;

#[path = "adapters/metrics/jsonl_sink.rs"]
mod jsonl_metrics_sink;

#[path = "adapters/memory/file.rs"]
mod memory_file;
