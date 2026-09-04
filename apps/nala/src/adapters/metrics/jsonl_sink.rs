//! Decorates an `EventSink` so every completed task is also appended as one
//! line of JSON to `events.jsonl` under `dir`, carrying everything
//! `CsvMetricsSink`'s three CSVs can't fit without breaking their escaping:
//! the original prompt, its source, the final reply, and each LLM/tool
//! call's full arguments and output. Meant to be wired alongside the CSV
//! sink, not instead of it — the CSVs stay the quick-aggregate view, this
//! is the detailed one for cost estimation and debugging.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Instant, SystemTime};

use serde::Serialize;

use crate::ports::events::{Event, EventSink, LlmCallId, RequestSource, TaskId};

use super::timestamp::format_rfc3339_utc;

const EVENTS_FILE: &str = "events.jsonl";

#[derive(Serialize)]
struct LlmCallRecord {
    call_index: u32,
    provider: String,
    model: String,
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
    latency_ms: u128,
    status: &'static str,
    error: Option<String>,
}

#[derive(Serialize)]
struct ToolCallRecord {
    index: u32,
    name: String,
    arguments: String,
    duration_ms: u128,
    status: &'static str,
    output: String,
    mutated: bool,
    error: Option<String>,
}

#[derive(Serialize)]
struct TaskRecord {
    task_id: TaskId,
    source: RequestSource,
    prompt: String,
    start_timestamp: String,
    end_timestamp: String,
    duration_ms: u128,
    status: &'static str,
    error: Option<String>,
    reply: Option<String>,
    llm_calls: Vec<LlmCallRecord>,
    tool_calls: Vec<ToolCallRecord>,
}

/// Position of an `LlmCompleted`'s record within its task's `llm_calls`, so
/// the `TokensUsed` that follows it can patch in the token counts. Recorded
/// eagerly at `LlmCompleted` (unlike `CsvMetricsSink`, which waits for
/// `TokensUsed` to write anything at all) so a backend that never reports
/// usage still leaves a record — just with `input_tokens`/`output_tokens`
/// left `None` — instead of the call vanishing entirely.
struct PendingTokens {
    task_id: TaskId,
    llm_call_id: LlmCallId,
    index: usize,
}

struct TaskAccumulator {
    prompt: String,
    source: RequestSource,
    start_timestamp: String,
    start_instant: Instant,
    reply: Option<String>,
    pending_error: Option<String>,
    llm_calls: Vec<LlmCallRecord>,
    tool_calls: Vec<ToolCallRecord>,
}

impl TaskAccumulator {
    fn new(prompt: String, source: RequestSource) -> Self {
        Self {
            prompt,
            source,
            start_timestamp: format_rfc3339_utc(SystemTime::now()),
            start_instant: Instant::now(),
            reply: None,
            pending_error: None,
            llm_calls: Vec::new(),
            tool_calls: Vec::new(),
        }
    }
}

pub struct JsonlMetricsSink<E> {
    inner: E,
    dir: Option<PathBuf>,
    tasks: HashMap<TaskId, TaskAccumulator>,
    pending_tokens: Option<PendingTokens>,
}

impl<E> JsonlMetricsSink<E> {
    pub fn new(inner: E, dir: Option<PathBuf>) -> Self {
        Self {
            inner,
            dir,
            tasks: HashMap::new(),
            pending_tokens: None,
        }
    }

    /// The wrapped sink, for callers (mainly tests) that need to inspect
    /// what was forwarded to it.
    pub fn inner(&self) -> &E {
        &self.inner
    }

    fn write_line(&self, record: &TaskRecord) {
        let Some(dir) = &self.dir else {
            return;
        };
        let path = dir.join(EVENTS_FILE);
        if let Err(error) = append_json_line(&path, record) {
            eprintln!(
                "Warning: could not write metrics line to {}: {error}",
                path.display()
            );
        }
    }

    fn finalize_task(&mut self, task_id: &TaskId, status: &'static str, error: Option<String>) {
        let Some(accumulator) = self.tasks.remove(task_id) else {
            return;
        };
        let error = error.or(accumulator.pending_error);
        let record = TaskRecord {
            task_id: task_id.clone(),
            source: accumulator.source,
            prompt: accumulator.prompt,
            start_timestamp: accumulator.start_timestamp,
            end_timestamp: format_rfc3339_utc(SystemTime::now()),
            duration_ms: accumulator.start_instant.elapsed().as_millis(),
            status,
            error,
            reply: accumulator.reply,
            llm_calls: accumulator.llm_calls,
            tool_calls: accumulator.tool_calls,
        };
        self.write_line(&record);
    }

    /// Same rationale as `CsvMetricsSink::flush_abandoned`: a task that
    /// aborted without ever emitting `RequestCompleted`/`Cancelled` would
    /// otherwise sit in `self.tasks` forever.
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

    fn record(&mut self, event: &Event) {
        match event {
            Event::RequestStarted {
                task_id,
                prompt,
                source,
            } => {
                self.flush_abandoned(Some(task_id));
                self.tasks.insert(
                    task_id.clone(),
                    TaskAccumulator::new(prompt.clone(), *source),
                );
            }
            Event::LlmCompleted {
                task_id,
                llm_call_id,
                call_index,
                duration,
                provider,
                model,
            } => {
                if let Some(accumulator) = self.tasks.get_mut(task_id) {
                    let index = accumulator.llm_calls.len();
                    accumulator.llm_calls.push(LlmCallRecord {
                        call_index: *call_index,
                        provider: provider.clone(),
                        model: model.clone(),
                        input_tokens: None,
                        output_tokens: None,
                        latency_ms: duration.as_millis(),
                        status: "ok",
                        error: None,
                    });
                    self.pending_tokens = Some(PendingTokens {
                        task_id: task_id.clone(),
                        llm_call_id: llm_call_id.clone(),
                        index,
                    });
                }
            }
            Event::TokensUsed {
                task_id,
                llm_call_id,
                prompt_tokens,
                completion_tokens,
                ..
            } => {
                let Some(pending) = self.pending_tokens.take() else {
                    return;
                };
                if pending.task_id != *task_id || pending.llm_call_id != *llm_call_id {
                    return;
                }
                if let Some(accumulator) = self.tasks.get_mut(task_id)
                    && let Some(call) = accumulator.llm_calls.get_mut(pending.index)
                {
                    call.input_tokens = *prompt_tokens;
                    call.output_tokens = *completion_tokens;
                }
            }
            Event::LlmFailed {
                task_id,
                call_index,
                duration,
                error,
                provider,
                model,
                ..
            } => {
                if let Some(accumulator) = self.tasks.get_mut(task_id) {
                    accumulator.llm_calls.push(LlmCallRecord {
                        call_index: *call_index,
                        provider: provider.clone(),
                        model: model.clone(),
                        input_tokens: None,
                        output_tokens: None,
                        latency_ms: duration.as_millis(),
                        status: "error",
                        error: Some(error.clone()),
                    });
                }
            }
            Event::ToolCompleted {
                task_id,
                tool_call_index,
                name,
                duration,
                output,
                arguments,
                mutated,
                ..
            } => {
                let is_error = output.starts_with("ERROR:");
                if let Some(accumulator) = self.tasks.get_mut(task_id) {
                    accumulator.tool_calls.push(ToolCallRecord {
                        index: *tool_call_index,
                        name: name.clone(),
                        arguments: arguments.clone(),
                        duration_ms: duration.as_millis(),
                        status: if is_error { "error" } else { "ok" },
                        output: output.clone(),
                        mutated: *mutated,
                        error: is_error.then(|| output.clone()),
                    });
                }
            }
            Event::RequestFailed { task_id, error, .. } => {
                if let Some(accumulator) = self.tasks.get_mut(task_id) {
                    accumulator.pending_error = Some(error.clone());
                }
            }
            Event::RequestCompleted { task_id, reply, .. } => {
                if let Some(accumulator) = self.tasks.get_mut(task_id) {
                    accumulator.reply = Some(reply.clone());
                }
                self.finalize_task(task_id, "ok", None);
            }
            Event::Cancelled { task_id } => {
                self.finalize_task(task_id, "cancelled", None);
            }
            _ => {}
        }
    }
}

impl<E: EventSink> EventSink for JsonlMetricsSink<E> {
    fn emit(&mut self, event: Event) {
        self.record(&event);
        self.inner.emit(event);
    }
}

impl<E> Drop for JsonlMetricsSink<E> {
    fn drop(&mut self) {
        self.flush_abandoned(None);
    }
}

fn append_json_line(path: &std::path::Path, record: &TaskRecord) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let json = serde_json::to_string(record)?;
    writeln!(file, "{json}")?;
    Ok(())
}
