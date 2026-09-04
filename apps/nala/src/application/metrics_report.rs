//! Turns `data/metrics/events.jsonl` (see `adapters::metrics::jsonl_sink`)
//! into an aggregate report: totals, cost estimated against a pricing
//! table, and rankings by tool and by request source. Pure over already-read
//! text/data so it's testable without touching the filesystem — `main.rs`
//! owns reading `events.jsonl` and `pricing.json` and printing the result.

use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct LlmCallLine {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ToolCallLine {
    name: String,
    status: String,
}

/// One line of `events.jsonl`. Only the fields the report needs are kept —
/// `prompt`/`reply`/`arguments`/`output` aren't, since no report metric
/// reads them today.
#[derive(Debug, Deserialize)]
pub struct TaskLine {
    source: String,
    duration_ms: u64,
    #[serde(default)]
    llm_calls: Vec<LlmCallLine>,
    #[serde(default)]
    tool_calls: Vec<ToolCallLine>,
}

/// Parses `events.jsonl` content, one `TaskLine` per line. A line that
/// isn't valid JSON (or doesn't match the shape) is skipped rather than
/// failing the whole file — the file is append-only and could in principle
/// have a partially-written last line from a crash mid-write.
pub fn parse_events(jsonl: &str) -> Vec<TaskLine> {
    jsonl
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct PricingEntry {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
}

pub type PricingTable = HashMap<String, PricingEntry>;

#[derive(Debug, Default)]
pub struct Report {
    pub total_tasks: usize,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub duration_p50_ms: u64,
    pub duration_p95_ms: u64,
    pub by_source: HashMap<String, usize>,
    pub tool_usage: HashMap<String, usize>,
    pub tool_errors: HashMap<String, usize>,
    /// Estimated USD cost of the actual token volume seen, had it all gone
    /// through each pricing entry's provider/model instead — an
    /// order-of-magnitude comparison, not a real bill: the token counts
    /// come from whatever tokenizer actually served the request (Ollama's,
    /// today), which isn't the tokenizer a cloud provider would have used.
    pub estimated_cost_usd: HashMap<String, f64>,
}

/// The `p`th percentile (0-100) of `values`, nearest-rank: `values[0]` is
/// the minimum, `values[len-1]` the maximum. Empty input yields 0 rather
/// than panicking, since an empty metrics file is a normal starting state.
fn percentile(mut values: Vec<u64>, p: f64) -> u64 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    let rank = ((p / 100.0) * values.len() as f64).ceil() as usize;
    let index = rank.saturating_sub(1).min(values.len() - 1);
    values[index]
}

pub fn build_report(tasks: &[TaskLine], pricing: &PricingTable) -> Report {
    let mut report = Report {
        total_tasks: tasks.len(),
        ..Report::default()
    };

    let mut durations = Vec::with_capacity(tasks.len());
    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;

    for task in tasks {
        *report.by_source.entry(task.source.clone()).or_insert(0) += 1;
        durations.push(task.duration_ms);

        for llm_call in &task.llm_calls {
            total_input_tokens += llm_call.input_tokens.unwrap_or(0);
            total_output_tokens += llm_call.output_tokens.unwrap_or(0);
        }

        for tool_call in &task.tool_calls {
            *report.tool_usage.entry(tool_call.name.clone()).or_insert(0) += 1;
            if tool_call.status == "error" {
                *report
                    .tool_errors
                    .entry(tool_call.name.clone())
                    .or_insert(0) += 1;
            }
        }
    }

    report.total_input_tokens = total_input_tokens;
    report.total_output_tokens = total_output_tokens;
    report.duration_p50_ms = percentile(durations.clone(), 50.0);
    report.duration_p95_ms = percentile(durations, 95.0);

    for (key, entry) in pricing {
        let cost = (total_input_tokens as f64 / 1_000_000.0) * entry.input_per_mtok
            + (total_output_tokens as f64 / 1_000_000.0) * entry.output_per_mtok;
        report.estimated_cost_usd.insert(key.clone(), cost);
    }

    report
}
