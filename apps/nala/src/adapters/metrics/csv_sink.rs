use std::collections::{BTreeSet, HashMap};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use crate::ports::events::{Event, EventSink, LlmCallId, TaskId};

use super::timestamp::format_rfc3339_utc;

const LLM_CALLS_FILE: &str = "llm_calls.csv";
const TOOL_CALLS_FILE: &str = "tool_calls.csv";
const TASKS_FILE: &str = "tasks.csv";

const LLM_CALLS_HEADER: &[&str] = &[
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
];

const TOOL_CALLS_HEADER: &[&str] = &[
    "timestamp",
    "task_id",
    "tool_call_index",
    "tool_name",
    "duration_ms",
    "status",
    "error",
];

const TASKS_HEADER: &[&str] = &[
    "task_id",
    "start_timestamp",
    "end_timestamp",
    "duration_ms",
    "llm_call_count",
    "input_tokens_total",
    "output_tokens_total",
    "total_tokens_total",
    "tool_call_count",
    "tools_used",
    "status",
    "error",
];

/// State the task an `LlmCompleted` belongs to, waiting for the
/// `TokensUsed` event that always immediately follows it (both are emitted
/// back-to-back, synchronously, from `agent_loop::instrumented_call_llm`),
/// so the two can be combined into one `llm_calls.csv` row.
struct PendingLlmCompletion {
    task_id: TaskId,
    llm_call_id: LlmCallId,
    duration: Duration,
}

/// Parameters for one `llm_calls.csv` row — grouped into a struct rather
/// than passed positionally, since a completed call alone already needs
/// nine of them.
struct LlmCallRow<'a> {
    task_id: &'a TaskId,
    llm_call_id: &'a LlmCallId,
    call_index: u32,
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
    total_tokens: Option<u32>,
    duration: Duration,
    status: &'a str,
    error: &'a str,
}

struct TaskAccumulator {
    start_timestamp: String,
    start_instant: Instant,
    llm_call_count: u32,
    input_tokens_total: Option<u64>,
    output_tokens_total: Option<u64>,
    total_tokens_total: Option<u64>,
    tool_call_count: u32,
    tools_used: BTreeSet<String>,
    /// Set by `RequestFailed`. Most `RequestFailed` events are terminal
    /// (loop detected, tool-call limit, ...), but one isn't — a non-fatal
    /// `speech.say` failure after a successful answer, which is always
    /// followed by `RequestCompleted` — so this is only a hint, consulted
    /// when the task turns out to have ended without ever completing or
    /// being cancelled (see `flush_abandoned`).
    pending_error: Option<String>,
}

impl TaskAccumulator {
    fn new() -> Self {
        Self {
            start_timestamp: format_rfc3339_utc(SystemTime::now()),
            start_instant: Instant::now(),
            llm_call_count: 0,
            input_tokens_total: None,
            output_tokens_total: None,
            total_tokens_total: None,
            tool_call_count: 0,
            tools_used: BTreeSet::new(),
            pending_error: None,
        }
    }
}

/// Decorates an `EventSink` so every LLM call, tool call, and completed
/// task is also appended as a row to one of three CSV files under `dir`
/// (`llm_calls.csv`, `tool_calls.csv`, `tasks.csv` — see
/// `apps/nala/src/plan.md` for the schema and rationale). Always forwards
/// the event to `inner` unchanged, same pattern as `SpeakingEventSink`.
///
/// `dir: None` makes every write a no-op — so this can be wired in
/// unconditionally without an env var forcing files onto disk during
/// development or tests.
///
/// I/O failures are swallowed (a warning printed once per failed write,
/// never propagated): the metrics system must never be able to break a
/// real task. `provider`/`model` are static, caller-supplied labels (Nala
/// wires exactly one `Llm` per run) rather than something read off
/// `Event`, since the event stream doesn't carry that — see `main.rs`.
pub struct CsvMetricsSink<E> {
    inner: E,
    dir: Option<PathBuf>,
    provider: String,
    model: String,
    tasks: HashMap<TaskId, TaskAccumulator>,
    pending_llm_completion: Option<PendingLlmCompletion>,
}

impl<E> CsvMetricsSink<E> {
    pub fn new(
        inner: E,
        dir: Option<PathBuf>,
        provider: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            inner,
            dir,
            provider: provider.into(),
            model: model.into(),
            tasks: HashMap::new(),
            pending_llm_completion: None,
        }
    }

    /// The wrapped sink, for callers (mainly tests) that need to inspect
    /// what was forwarded to it.
    pub fn inner(&self) -> &E {
        &self.inner
    }

    fn append_row(&self, file: &str, header: &[&str], row: &[String]) {
        let Some(dir) = &self.dir else {
            return;
        };
        let path = dir.join(file);
        if let Err(error) = write_row(&path, header, row) {
            eprintln!(
                "Warning: could not write metrics row to {}: {error}",
                path.display()
            );
        }
    }

    fn write_llm_call_row(&self, call: LlmCallRow<'_>) {
        let row = vec![
            format_rfc3339_utc(SystemTime::now()),
            call.task_id.to_string(),
            call.llm_call_id.to_string(),
            call.call_index.to_string(),
            self.provider.clone(),
            self.model.clone(),
            opt_to_string(call.input_tokens),
            opt_to_string(call.output_tokens),
            opt_to_string(call.total_tokens),
            call.duration.as_millis().to_string(),
            String::new(), // time_to_first_token_ms: no streaming support yet
            call.status.to_string(),
            call.error.to_string(),
        ];
        self.append_row(LLM_CALLS_FILE, LLM_CALLS_HEADER, &row);
    }

    fn write_tool_call_row(
        &self,
        task_id: &TaskId,
        tool_call_index: u32,
        name: &str,
        duration: Duration,
        status: &str,
        error: &str,
    ) {
        let row = vec![
            format_rfc3339_utc(SystemTime::now()),
            task_id.to_string(),
            tool_call_index.to_string(),
            name.to_string(),
            duration.as_millis().to_string(),
            status.to_string(),
            error.to_string(),
        ];
        self.append_row(TOOL_CALLS_FILE, TOOL_CALLS_HEADER, &row);
    }

    /// Removes `task_id`'s accumulator (if any — a task that never emitted
    /// `RequestStarted` through this sink, impossible in practice, is
    /// silently ignored) and writes its `tasks.csv` row.
    fn finalize_task(&mut self, task_id: &TaskId, status: &str, error: Option<String>) {
        let Some(accumulator) = self.tasks.remove(task_id) else {
            return;
        };

        let duration_ms = accumulator.start_instant.elapsed().as_millis();
        let end_timestamp = format_rfc3339_utc(SystemTime::now());
        let tools_used = accumulator
            .tools_used
            .into_iter()
            .collect::<Vec<_>>()
            .join(";");

        let row = vec![
            task_id.to_string(),
            accumulator.start_timestamp,
            end_timestamp,
            duration_ms.to_string(),
            accumulator.llm_call_count.to_string(),
            opt_u64_to_string(accumulator.input_tokens_total),
            opt_u64_to_string(accumulator.output_tokens_total),
            opt_u64_to_string(accumulator.total_tokens_total),
            accumulator.tool_call_count.to_string(),
            tools_used,
            status.to_string(),
            error.unwrap_or_default(),
        ];
        self.append_row(TASKS_FILE, TASKS_HEADER, &row);
    }

    /// Finalizes every task other than `except` as `status=error` — covers
    /// a task that aborted (loop detected, tool-call limit, ...) without
    /// ever emitting `RequestCompleted`/`Cancelled`, whose accumulator
    /// would otherwise sit forever. Called when a new task starts (proof
    /// the previous one's `process()` call has returned, since Nala runs
    /// one task at a time) and once more from `Drop`, to catch the very
    /// last task of a session that ends this way.
    fn flush_abandoned(&mut self, except: Option<&TaskId>) {
        let stale: Vec<TaskId> = self
            .tasks
            .keys()
            .filter(|task_id| Some(*task_id) != except)
            .cloned()
            .collect();

        for task_id in stale {
            let error = self
                .tasks
                .get(&task_id)
                .and_then(|accumulator| accumulator.pending_error.clone())
                .unwrap_or_else(|| "task ended without a terminal event".to_string());
            self.finalize_task(&task_id, "error", Some(error));
        }
    }

    fn handle_tokens_used(
        &mut self,
        task_id: &TaskId,
        llm_call_id: &LlmCallId,
        call_index: u32,
        prompt_tokens: Option<u32>,
        completion_tokens: Option<u32>,
    ) {
        let Some(pending) = self.pending_llm_completion.take() else {
            return;
        };
        // Guards the pairing invariant (see `PendingLlmCompletion`) rather
        // than trusting it blindly — if it's ever violated, drop the
        // mismatched pending entry instead of writing a wrong row.
        if pending.task_id != *task_id || pending.llm_call_id != *llm_call_id {
            return;
        }

        let total_tokens = prompt_tokens.zip(completion_tokens).map(|(p, c)| p + c);

        self.write_llm_call_row(LlmCallRow {
            task_id,
            llm_call_id,
            call_index,
            input_tokens: prompt_tokens,
            output_tokens: completion_tokens,
            total_tokens,
            duration: pending.duration,
            status: "ok",
            error: "",
        });

        if let Some(accumulator) = self.tasks.get_mut(task_id) {
            accumulator.llm_call_count += 1;
            add_optional_u64(&mut accumulator.input_tokens_total, prompt_tokens);
            add_optional_u64(&mut accumulator.output_tokens_total, completion_tokens);
            add_optional_u64(&mut accumulator.total_tokens_total, total_tokens);
        }
    }

    fn record(&mut self, event: &Event) {
        match event {
            Event::RequestStarted { task_id } => {
                self.flush_abandoned(Some(task_id));
                self.tasks.insert(task_id.clone(), TaskAccumulator::new());
            }
            Event::LlmCompleted {
                task_id,
                llm_call_id,
                duration,
                ..
            } => {
                self.pending_llm_completion = Some(PendingLlmCompletion {
                    task_id: task_id.clone(),
                    llm_call_id: llm_call_id.clone(),
                    duration: *duration,
                });
            }
            Event::TokensUsed {
                task_id,
                llm_call_id,
                call_index,
                prompt_tokens,
                completion_tokens,
            } => {
                self.handle_tokens_used(
                    task_id,
                    llm_call_id,
                    *call_index,
                    *prompt_tokens,
                    *completion_tokens,
                );
            }
            Event::LlmFailed {
                task_id,
                llm_call_id,
                call_index,
                duration,
                error,
            } => {
                self.write_llm_call_row(LlmCallRow {
                    task_id,
                    llm_call_id,
                    call_index: *call_index,
                    input_tokens: None,
                    output_tokens: None,
                    total_tokens: None,
                    duration: *duration,
                    status: "error",
                    error,
                });
                if let Some(accumulator) = self.tasks.get_mut(task_id) {
                    accumulator.llm_call_count += 1;
                }
            }
            Event::ToolCompleted {
                task_id,
                tool_call_index,
                name,
                duration,
                output,
                ..
            } => {
                let is_error = output.starts_with("ERROR:");
                let status = if is_error { "error" } else { "ok" };
                let error = if is_error { output.as_str() } else { "" };
                self.write_tool_call_row(task_id, *tool_call_index, name, *duration, status, error);

                if let Some(accumulator) = self.tasks.get_mut(task_id) {
                    accumulator.tool_call_count += 1;
                    accumulator.tools_used.insert(name.clone());
                }
            }
            Event::RequestFailed { task_id, error, .. } => {
                if let Some(accumulator) = self.tasks.get_mut(task_id) {
                    accumulator.pending_error = Some(error.clone());
                }
            }
            Event::RequestCompleted { task_id, .. } => {
                self.finalize_task(task_id, "ok", None);
            }
            Event::Cancelled { task_id } => {
                self.finalize_task(task_id, "cancelled", None);
            }
            _ => {}
        }
    }
}

impl<E: EventSink> EventSink for CsvMetricsSink<E> {
    fn emit(&mut self, event: Event) {
        self.record(&event);
        self.inner.emit(event);
    }
}

impl<E> Drop for CsvMetricsSink<E> {
    fn drop(&mut self) {
        self.flush_abandoned(None);
    }
}

fn opt_to_string(value: Option<u32>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

fn opt_u64_to_string(value: Option<u64>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

fn add_optional_u64(accumulator: &mut Option<u64>, value: Option<u32>) {
    if let Some(value) = value {
        *accumulator = Some(accumulator.unwrap_or(0) + value as u64);
    }
}

fn write_row(path: &Path, header: &[&str], row: &[String]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let needs_header = match std::fs::metadata(path) {
        Ok(metadata) => metadata.len() == 0,
        Err(_) => true,
    };

    let file = OpenOptions::new().create(true).append(true).open(path)?;
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(file);
    if needs_header {
        writer.write_record(header)?;
    }
    writer.write_record(row)?;
    writer.flush()?;
    Ok(())
}
