use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use nala::adapters::metrics::jsonl_sink::JsonlMetricsSink;
use nala::ports::events::{Event, EventSink, LlmCallId, RequestSource, TaskId};
use serde_json::Value;

use crate::fake_events::RecordingEventSink;

/// A fresh, empty directory per test, same rationale as the CSV sink's
/// tests: `write_line` creates the directory itself, but a unique one per
/// test keeps each test's file inspectable in isolation.
fn temp_dir() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "nala_jsonl_metrics_test_{}_{n}_{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn read_lines(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()))
        .lines()
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("line was not valid JSON: {error}\nline: {line}"))
        })
        .collect()
}

fn sink(dir: &Path) -> JsonlMetricsSink<RecordingEventSink> {
    JsonlMetricsSink::new(RecordingEventSink::new(), Some(dir.to_path_buf()))
}

#[test]
fn a_completed_task_writes_one_line_with_prompt_source_and_reply() {
    let dir = temp_dir();
    let mut sink = sink(&dir);
    let task_id = TaskId::new();

    sink.emit(Event::RequestStarted {
        task_id: task_id.clone(),
        prompt: "que hora es?".to_string(),
        source: RequestSource::Android,
    });
    sink.emit(Event::RequestCompleted {
        task_id: task_id.clone(),
        duration: Duration::from_millis(120),
        reply: "son las 10".to_string(),
    });

    let lines = read_lines(&dir.join("events.jsonl"));
    assert_eq!(lines.len(), 1);
    let record = &lines[0];
    assert_eq!(record["task_id"], task_id.to_string());
    assert_eq!(record["source"], "android");
    assert_eq!(record["prompt"], "que hora es?");
    assert_eq!(record["reply"], "son las 10");
    assert_eq!(record["status"], "ok");
    assert!(record["duration_ms"].is_number());
    assert!(record["error"].is_null());
}

#[test]
fn an_llm_call_is_recorded_even_when_no_tokens_used_event_ever_arrives() {
    // Regression guard: a backend that never reports usage (plausible for
    // a future cloud provider) must not silently produce zero llm call
    // records, unlike the CSV sink's TokensUsed-gated write.
    let dir = temp_dir();
    let mut sink = sink(&dir);
    let task_id = TaskId::new();
    let llm_call_id = LlmCallId::new(&task_id, 1);

    sink.emit(Event::RequestStarted {
        task_id: task_id.clone(),
        prompt: "hola".to_string(),
        source: RequestSource::Cli,
    });
    sink.emit(Event::LlmStarted {
        task_id: task_id.clone(),
        llm_call_id: llm_call_id.clone(),
        call_index: 1,
        images: 0,
        provider: "ollama".to_string(),
        model: "gemma4:12b".to_string(),
    });
    sink.emit(Event::LlmCompleted {
        task_id: task_id.clone(),
        llm_call_id,
        call_index: 1,
        duration: Duration::from_millis(80),
        provider: "ollama".to_string(),
        model: "gemma4:12b".to_string(),
    });
    // No TokensUsed here.
    sink.emit(Event::RequestCompleted {
        task_id: task_id.clone(),
        duration: Duration::from_millis(90),
        reply: "listo".to_string(),
    });

    let lines = read_lines(&dir.join("events.jsonl"));
    let llm_calls = lines[0]["llm_calls"].as_array().unwrap();
    assert_eq!(llm_calls.len(), 1);
    assert_eq!(llm_calls[0]["provider"], "ollama");
    assert_eq!(llm_calls[0]["model"], "gemma4:12b");
    assert_eq!(llm_calls[0]["latency_ms"], 80);
    assert!(llm_calls[0]["input_tokens"].is_null());
    assert!(llm_calls[0]["output_tokens"].is_null());
}

#[test]
fn tokens_used_fills_in_the_matching_llm_calls_token_counts() {
    let dir = temp_dir();
    let mut sink = sink(&dir);
    let task_id = TaskId::new();
    let llm_call_id = LlmCallId::new(&task_id, 1);

    sink.emit(Event::RequestStarted {
        task_id: task_id.clone(),
        prompt: "hola".to_string(),
        source: RequestSource::Cli,
    });
    sink.emit(Event::LlmCompleted {
        task_id: task_id.clone(),
        llm_call_id: llm_call_id.clone(),
        call_index: 1,
        duration: Duration::from_millis(80),
        provider: "ollama".to_string(),
        model: "gemma4:12b".to_string(),
    });
    sink.emit(Event::TokensUsed {
        task_id: task_id.clone(),
        llm_call_id,
        call_index: 1,
        prompt_tokens: Some(50),
        completion_tokens: Some(20),
    });
    sink.emit(Event::RequestCompleted {
        task_id: task_id.clone(),
        duration: Duration::from_millis(90),
        reply: "listo".to_string(),
    });

    let lines = read_lines(&dir.join("events.jsonl"));
    let llm_calls = lines[0]["llm_calls"].as_array().unwrap();
    assert_eq!(llm_calls[0]["input_tokens"], 50);
    assert_eq!(llm_calls[0]["output_tokens"], 20);
}

#[test]
fn tool_calls_carry_their_arguments_and_output() {
    let dir = temp_dir();
    let mut sink = sink(&dir);
    let task_id = TaskId::new();

    sink.emit(Event::RequestStarted {
        task_id: task_id.clone(),
        prompt: "clima en cali".to_string(),
        source: RequestSource::Cli,
    });
    sink.emit(Event::ToolStarted {
        task_id: task_id.clone(),
        tool_call_index: 1,
        name: "get_weather".to_string(),
        arguments: "{\"city\":\"Cali\"}".to_string(),
    });
    sink.emit(Event::ToolCompleted {
        task_id: task_id.clone(),
        tool_call_index: 1,
        name: "get_weather".to_string(),
        duration: Duration::from_millis(500),
        output: "soleado".to_string(),
        images: 0,
        arguments: "{\"city\":\"Cali\"}".to_string(),
        mutated: false,
    });
    sink.emit(Event::RequestCompleted {
        task_id: task_id.clone(),
        duration: Duration::from_millis(600),
        reply: "esta soleado".to_string(),
    });

    let lines = read_lines(&dir.join("events.jsonl"));
    let tool_calls = lines[0]["tool_calls"].as_array().unwrap();
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0]["name"], "get_weather");
    assert_eq!(tool_calls[0]["arguments"], "{\"city\":\"Cali\"}");
    assert_eq!(tool_calls[0]["output"], "soleado");
    assert_eq!(tool_calls[0]["mutated"], false);
    assert_eq!(tool_calls[0]["status"], "ok");
}

#[test]
fn an_aborted_task_is_flushed_with_error_status_on_drop() {
    let dir = temp_dir();
    let task_id = TaskId::new();
    {
        let mut sink = sink(&dir);
        sink.emit(Event::RequestStarted {
            task_id: task_id.clone(),
            prompt: "hola".to_string(),
            source: RequestSource::Cli,
        });
        sink.emit(Event::RequestFailed {
            task_id: task_id.clone(),
            duration: Duration::from_millis(5),
            error: "tool call limit exceeded".to_string(),
        });
        // Dropped here with no RequestCompleted/Cancelled ever arriving.
    }

    let lines = read_lines(&dir.join("events.jsonl"));
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["status"], "error");
    assert_eq!(lines[0]["error"], "tool call limit exceeded");
    assert!(lines[0]["reply"].is_null());
}

#[test]
fn a_second_session_appends_instead_of_overwriting() {
    let dir = temp_dir();
    let task_a = TaskId::new();
    {
        let mut sink = sink(&dir);
        sink.emit(Event::RequestStarted {
            task_id: task_a.clone(),
            prompt: "hola".to_string(),
            source: RequestSource::Cli,
        });
        sink.emit(Event::RequestCompleted {
            task_id: task_a.clone(),
            duration: Duration::from_millis(10),
            reply: "ok".to_string(),
        });
    }

    let task_b = TaskId::new();
    {
        let mut sink = sink(&dir);
        sink.emit(Event::RequestStarted {
            task_id: task_b.clone(),
            prompt: "hola de nuevo".to_string(),
            source: RequestSource::Cli,
        });
        sink.emit(Event::RequestCompleted {
            task_id: task_b.clone(),
            duration: Duration::from_millis(10),
            reply: "ok".to_string(),
        });
    }

    let lines = read_lines(&dir.join("events.jsonl"));
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["task_id"], task_a.to_string());
    assert_eq!(lines[1]["task_id"], task_b.to_string());
}
