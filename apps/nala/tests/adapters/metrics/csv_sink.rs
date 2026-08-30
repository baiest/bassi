use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use nala::adapters::metrics::csv_sink::CsvMetricsSink;
use nala::ports::events::{Event, EventSink, LlmCallId, TaskId};

use crate::fake_events::RecordingEventSink;

/// A fresh, empty directory per test — no external tempfile crate needed
/// (see `write_row` in `csv_sink.rs`: it creates the directory itself if
/// missing, so tests don't even have to pre-create it, but doing so here
/// keeps each test's directory unique and inspectable).
fn temp_dir() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "nala_csv_metrics_test_{}_{n}_{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn read_csv(path: &Path) -> Vec<Vec<String>> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_path(path)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));
    reader
        .records()
        .map(|record| {
            record
                .unwrap()
                .iter()
                .map(|field| field.to_string())
                .collect()
        })
        .collect()
}

fn sink(dir: &Path) -> CsvMetricsSink<RecordingEventSink> {
    CsvMetricsSink::new(
        RecordingEventSink::new(),
        Some(dir.to_path_buf()),
        "ollama",
        "gemma4:12b",
    )
}

fn started(task_id: &TaskId) -> Event {
    Event::RequestStarted {
        task_id: task_id.clone(),
    }
}

fn completed(task_id: &TaskId, ms: u64) -> Event {
    Event::RequestCompleted {
        task_id: task_id.clone(),
        duration: Duration::from_millis(ms),
    }
}

fn llm_call(
    task_id: &TaskId,
    call_index: u32,
    latency_ms: u64,
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
) -> [Event; 3] {
    let llm_call_id = LlmCallId::new(task_id, call_index);
    [
        Event::LlmStarted {
            task_id: task_id.clone(),
            llm_call_id: llm_call_id.clone(),
            call_index,
            images: 0,
        },
        Event::LlmCompleted {
            task_id: task_id.clone(),
            llm_call_id: llm_call_id.clone(),
            call_index,
            duration: Duration::from_millis(latency_ms),
        },
        Event::TokensUsed {
            task_id: task_id.clone(),
            llm_call_id,
            call_index,
            prompt_tokens,
            completion_tokens,
        },
    ]
}

#[test]
fn a_completed_llm_call_records_its_exact_token_usage() {
    let dir = temp_dir();
    let mut sink = sink(&dir);
    let task_id = TaskId::new();

    sink.emit(started(&task_id));
    for event in llm_call(&task_id, 1, 120, Some(50), Some(20)) {
        sink.emit(event);
    }
    sink.emit(completed(&task_id, 200));

    let rows = read_csv(&dir.join("llm_calls.csv"));
    assert_eq!(rows.len(), 2, "expected a header row and one data row");
    assert_eq!(
        rows[0],
        vec![
            "timestamp",
            "task_id",
            "llm_call_id",
            "call_index",
            "provider",
            "model",
            "input_tokens",
            "output_tokens",
            "total_tokens",
            "latency_ms",
            "time_to_first_token_ms",
            "status",
            "error",
        ]
    );
    let row = &rows[1];
    assert_eq!(row[1], task_id.to_string());
    assert_eq!(row[3], "1"); // call_index
    assert_eq!(row[4], "ollama");
    assert_eq!(row[5], "gemma4:12b");
    assert_eq!(row[6], "50"); // input_tokens
    assert_eq!(row[7], "20"); // output_tokens
    assert_eq!(row[8], "70"); // total_tokens
    assert_eq!(row[9], "120"); // latency_ms
    assert_eq!(row[10], ""); // time_to_first_token_ms: no streaming yet
    assert_eq!(row[11], "ok");
    assert_eq!(row[12], "");
}

#[test]
fn a_task_with_one_llm_call_generates_correct_task_metrics() {
    let dir = temp_dir();
    let mut sink = sink(&dir);
    let task_id = TaskId::new();

    sink.emit(started(&task_id));
    for event in llm_call(&task_id, 1, 50, Some(50), Some(20)) {
        sink.emit(event);
    }
    sink.emit(completed(&task_id, 100));

    let rows = read_csv(&dir.join("tasks.csv"));
    assert_eq!(rows.len(), 2);
    let row = &rows[1];
    assert_eq!(row[0], task_id.to_string());
    assert_eq!(row[4], "1"); // llm_call_count
    assert_eq!(row[5], "50"); // input_tokens_total
    assert_eq!(row[6], "20"); // output_tokens_total
    assert_eq!(row[7], "70"); // total_tokens_total
    assert_eq!(row[8], "0"); // tool_call_count
    assert_eq!(row[9], ""); // tools_used
    assert_eq!(row[10], "ok");
}

#[test]
fn a_task_with_multiple_llm_calls_accumulates_tokens_correctly() {
    let dir = temp_dir();
    let mut sink = sink(&dir);
    let task_id = TaskId::new();

    sink.emit(started(&task_id));
    for event in llm_call(&task_id, 1, 10, Some(10), Some(5)) {
        sink.emit(event);
    }
    for event in llm_call(&task_id, 2, 10, Some(20), Some(8)) {
        sink.emit(event);
    }
    for event in llm_call(&task_id, 3, 10, Some(30), Some(12)) {
        sink.emit(event);
    }
    sink.emit(completed(&task_id, 100));

    let rows = read_csv(&dir.join("tasks.csv"));
    let row = &rows[1];
    assert_eq!(row[4], "3"); // llm_call_count
    assert_eq!(row[5], "60"); // input_tokens_total = 10+20+30
    assert_eq!(row[6], "25"); // output_tokens_total = 5+8+12
    assert_eq!(row[7], "85"); // total_tokens_total
}

#[test]
fn tool_calls_are_tagged_with_their_tasks_id() {
    let dir = temp_dir();
    let mut sink = sink(&dir);
    let task_id = TaskId::new();

    sink.emit(started(&task_id));
    sink.emit(Event::ToolStarted {
        task_id: task_id.clone(),
        tool_call_index: 1,
        name: "execute_command".to_string(),
        arguments: "{}".to_string(),
    });
    sink.emit(Event::ToolCompleted {
        task_id: task_id.clone(),
        tool_call_index: 1,
        name: "execute_command".to_string(),
        duration: Duration::from_millis(15),
        output: "ok".to_string(),
        images: 0,
    });
    sink.emit(completed(&task_id, 50));

    let rows = read_csv(&dir.join("tool_calls.csv"));
    assert_eq!(rows.len(), 2);
    let row = &rows[1];
    assert_eq!(row[1], task_id.to_string());
    assert_eq!(row[2], "1"); // tool_call_index
    assert_eq!(row[3], "execute_command");
    assert_eq!(row[4], "15"); // duration_ms
    assert_eq!(row[5], "ok");

    let task_rows = read_csv(&dir.join("tasks.csv"));
    assert_eq!(task_rows[1][8], "1"); // tool_call_count
    assert_eq!(task_rows[1][9], "execute_command"); // tools_used
}

#[test]
fn a_second_session_appends_instead_of_overwriting() {
    let dir = temp_dir();
    let task_a = TaskId::new();
    {
        let mut sink = sink(&dir);
        sink.emit(started(&task_a));
        sink.emit(completed(&task_a, 10));
    }

    let task_b = TaskId::new();
    {
        // A brand new sink instance, as if Nala had been restarted — same
        // directory, no shared in-memory state.
        let mut sink = sink(&dir);
        sink.emit(started(&task_b));
        sink.emit(completed(&task_b, 10));
    }

    let rows = read_csv(&dir.join("tasks.csv"));
    assert_eq!(rows.len(), 3, "expected one header row and two data rows");
    assert_eq!(rows[1][0], task_a.to_string());
    assert_eq!(rows[2][0], task_b.to_string());
}

#[test]
fn a_failed_llm_call_is_recorded_with_its_error() {
    let dir = temp_dir();
    let mut sink = sink(&dir);
    let task_id = TaskId::new();
    let llm_call_id = LlmCallId::new(&task_id, 1);

    sink.emit(started(&task_id));
    sink.emit(Event::LlmStarted {
        task_id: task_id.clone(),
        llm_call_id: llm_call_id.clone(),
        call_index: 1,
        images: 0,
    });
    sink.emit(Event::LlmFailed {
        task_id: task_id.clone(),
        llm_call_id,
        call_index: 1,
        duration: Duration::from_millis(30),
        error: "connection refused".to_string(),
    });

    let rows = read_csv(&dir.join("llm_calls.csv"));
    let row = &rows[1];
    assert_eq!(row[6], ""); // input_tokens unknown
    assert_eq!(row[7], ""); // output_tokens unknown
    assert_eq!(row[8], ""); // total_tokens unknown
    assert_eq!(row[11], "error");
    assert_eq!(row[12], "connection refused");
}

#[test]
fn a_failing_tool_call_is_recorded_and_the_task_still_completes() {
    let dir = temp_dir();
    let mut sink = sink(&dir);
    let task_id = TaskId::new();

    sink.emit(started(&task_id));
    sink.emit(Event::ToolStarted {
        task_id: task_id.clone(),
        tool_call_index: 1,
        name: "execute_command".to_string(),
        arguments: "{}".to_string(),
    });
    sink.emit(Event::ToolCompleted {
        task_id: task_id.clone(),
        tool_call_index: 1,
        name: "execute_command".to_string(),
        duration: Duration::from_millis(5),
        output: "ERROR: boom".to_string(),
        images: 0,
    });
    sink.emit(completed(&task_id, 20));

    let tool_rows = read_csv(&dir.join("tool_calls.csv"));
    assert_eq!(tool_rows[1][5], "error");
    assert_eq!(tool_rows[1][6], "ERROR: boom");

    let task_rows = read_csv(&dir.join("tasks.csv"));
    assert_eq!(task_rows[1][10], "ok", "the task itself still completed");
}

#[test]
fn a_cancelled_task_leaves_already_written_rows_intact() {
    let dir = temp_dir();
    let mut sink = sink(&dir);
    let task_id = TaskId::new();

    sink.emit(started(&task_id));
    for event in llm_call(&task_id, 1, 10, Some(5), Some(5)) {
        sink.emit(event);
    }
    sink.emit(Event::Cancelled {
        task_id: task_id.clone(),
    });
    // `cancelled()` in agent_loop.rs always emits `RequestFailed` right
    // after `Cancelled` — the sink must not double-write or panic on it.
    sink.emit(Event::RequestFailed {
        task_id: task_id.clone(),
        duration: Duration::from_millis(15),
        error: "cancelled".to_string(),
    });

    let llm_rows = read_csv(&dir.join("llm_calls.csv"));
    assert_eq!(
        llm_rows.len(),
        2,
        "the call made before cancellation survives"
    );

    let task_rows = read_csv(&dir.join("tasks.csv"));
    assert_eq!(task_rows.len(), 2, "exactly one tasks.csv row, not two");
    assert_eq!(task_rows[1][10], "cancelled");
}

#[test]
fn missing_token_usage_does_not_break_normal_task_completion() {
    let dir = temp_dir();
    let mut sink = sink(&dir);
    let task_id = TaskId::new();

    sink.emit(started(&task_id));
    for event in llm_call(&task_id, 1, 10, None, None) {
        sink.emit(event);
    }
    sink.emit(completed(&task_id, 20));

    let llm_rows = read_csv(&dir.join("llm_calls.csv"));
    let row = &llm_rows[1];
    assert_eq!(row[6], "");
    assert_eq!(row[7], "");
    assert_eq!(row[8], "");
    assert_eq!(row[11], "ok");

    let task_rows = read_csv(&dir.join("tasks.csv"));
    let row = &task_rows[1];
    assert_eq!(row[4], "1"); // llm_call_count still counted
    assert_eq!(row[5], ""); // input_tokens_total: never reported, not zero
    assert_eq!(row[6], "");
    assert_eq!(row[7], "");
    assert_eq!(row[10], "ok");
}

#[test]
fn a_task_that_aborts_without_a_terminal_event_is_flushed_when_the_next_one_starts() {
    let dir = temp_dir();
    let mut sink = sink(&dir);
    let task_a = TaskId::new();

    // Mirrors `abort()` in agent_loop.rs: RequestStarted, then some
    // activity, then RequestFailed with nothing after it (no
    // RequestCompleted, no Cancelled) — e.g. a loop-detected abort.
    sink.emit(started(&task_a));
    sink.emit(Event::RequestFailed {
        task_id: task_a.clone(),
        duration: Duration::from_millis(30),
        error: "loop detected".to_string(),
    });

    let task_b = TaskId::new();
    sink.emit(started(&task_b));
    sink.emit(completed(&task_b, 10));

    let rows = read_csv(&dir.join("tasks.csv"));
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[1][0], task_a.to_string());
    assert_eq!(rows[1][10], "error");
    assert_eq!(rows[1][11], "loop detected");
    assert_eq!(rows[2][0], task_b.to_string());
    assert_eq!(rows[2][10], "ok");
}

#[test]
fn dropping_the_sink_flushes_the_last_abandoned_task() {
    let dir = temp_dir();
    let task_id = TaskId::new();
    {
        let mut sink = sink(&dir);
        sink.emit(started(&task_id));
        sink.emit(Event::RequestFailed {
            task_id: task_id.clone(),
            duration: Duration::from_millis(5),
            error: "tool call limit exceeded".to_string(),
        });
        // sink dropped here, with no RequestCompleted/Cancelled ever
        // having arrived for this task.
    }

    let rows = read_csv(&dir.join("tasks.csv"));
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1][0], task_id.to_string());
    assert_eq!(rows[1][10], "error");
}

#[test]
fn without_a_configured_directory_nothing_is_written_and_events_still_forward() {
    let mut sink = CsvMetricsSink::new(RecordingEventSink::new(), None, "ollama", "gemma4:12b");
    let task_id = TaskId::new();

    sink.emit(started(&task_id));
    for event in llm_call(&task_id, 1, 10, Some(5), Some(5)) {
        sink.emit(event);
    }
    sink.emit(completed(&task_id, 20));

    assert_eq!(sink.inner().events.len(), 5);
}
