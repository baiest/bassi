use std::collections::HashMap;

use nala::application::metrics_report::{PricingEntry, build_report, parse_events};

fn task_line(
    source: &str,
    duration_ms: u64,
    tool_names: &[&str],
    provider: &str,
    model: &str,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
) -> String {
    let tool_calls: Vec<String> = tool_names
        .iter()
        .map(|name| {
            format!(
                r#"{{"index":1,"name":"{name}","arguments":"{{}}","duration_ms":10,"status":"ok","output":"ok","mutated":false,"error":null}}"#
            )
        })
        .collect();
    let tool_calls_json = tool_calls.join(",");
    let input = input_tokens
        .map(|v| v.to_string())
        .unwrap_or_else(|| "null".to_string());
    let output = output_tokens
        .map(|v| v.to_string())
        .unwrap_or_else(|| "null".to_string());

    format!(
        r#"{{"task_id":"t1","source":"{source}","prompt":"hola","start_timestamp":"2026-01-01T00:00:00Z","end_timestamp":"2026-01-01T00:00:01Z","duration_ms":{duration_ms},"status":"ok","error":null,"reply":"listo","llm_calls":[{{"call_index":1,"provider":"{provider}","model":"{model}","input_tokens":{input},"output_tokens":{output},"latency_ms":50,"status":"ok","error":null}}],"tool_calls":[{tool_calls_json}]}}"#
    )
}

#[test]
fn parse_events_skips_malformed_lines_instead_of_failing_the_whole_file() {
    let jsonl = format!(
        "{}\nnot valid json\n{}\n",
        task_line("cli", 100, &[], "ollama", "gemma4:12b", Some(10), Some(5)),
        task_line(
            "android",
            200,
            &[],
            "ollama",
            "gemma4:12b",
            Some(20),
            Some(8)
        ),
    );

    let tasks = parse_events(&jsonl);

    assert_eq!(tasks.len(), 2);
}

#[test]
fn build_report_totals_tasks_and_tokens() {
    let jsonl = format!(
        "{}\n{}\n",
        task_line("cli", 100, &[], "ollama", "gemma4:12b", Some(10), Some(5)),
        task_line(
            "android",
            200,
            &[],
            "ollama",
            "gemma4:12b",
            Some(20),
            Some(8)
        ),
    );
    let tasks = parse_events(&jsonl);

    let report = build_report(&tasks, &HashMap::new());

    assert_eq!(report.total_tasks, 2);
    assert_eq!(report.total_input_tokens, 30);
    assert_eq!(report.total_output_tokens, 13);
}

#[test]
fn build_report_breaks_down_tasks_by_source() {
    let jsonl = format!(
        "{}\n{}\n{}\n",
        task_line("cli", 100, &[], "ollama", "gemma4:12b", Some(10), Some(5)),
        task_line(
            "android",
            200,
            &[],
            "ollama",
            "gemma4:12b",
            Some(10),
            Some(5)
        ),
        task_line(
            "android",
            200,
            &[],
            "ollama",
            "gemma4:12b",
            Some(10),
            Some(5)
        ),
    );
    let tasks = parse_events(&jsonl);

    let report = build_report(&tasks, &HashMap::new());

    assert_eq!(report.by_source.get("cli").copied(), Some(1));
    assert_eq!(report.by_source.get("android").copied(), Some(2));
}

#[test]
fn build_report_ranks_tools_by_call_count() {
    let jsonl = format!(
        "{}\n{}\n",
        task_line(
            "cli",
            100,
            &["get_weather", "web_search"],
            "ollama",
            "gemma4:12b",
            Some(10),
            Some(5)
        ),
        task_line(
            "cli",
            100,
            &["get_weather"],
            "ollama",
            "gemma4:12b",
            Some(10),
            Some(5)
        ),
    );
    let tasks = parse_events(&jsonl);

    let report = build_report(&tasks, &HashMap::new());

    assert_eq!(report.tool_usage.get("get_weather").copied(), Some(2));
    assert_eq!(report.tool_usage.get("web_search").copied(), Some(1));
}

#[test]
fn build_report_estimates_cost_per_pricing_entry() {
    let jsonl = task_line(
        "cli",
        100,
        &[],
        "ollama",
        "gemma4:12b",
        Some(1_000_000),
        Some(1_000_000),
    ) + "\n";
    let tasks = parse_events(&jsonl);

    let mut pricing = HashMap::new();
    pricing.insert(
        "bedrock/anthropic.claude-sonnet-4".to_string(),
        PricingEntry {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
        },
    );

    let report = build_report(&tasks, &pricing);

    let cost = report
        .estimated_cost_usd
        .get("bedrock/anthropic.claude-sonnet-4")
        .copied()
        .expect("a cost estimate for the priced entry");
    // 1M input tokens * $3/Mtok + 1M output tokens * $15/Mtok = $18, applied
    // to the actual token volume regardless of which provider produced it
    // -- an estimate of "what if this load had gone through this backend."
    assert!((cost - 18.0).abs() < 1e-9, "expected 18.0, got {cost}");
}

#[test]
fn build_report_computes_duration_percentiles() {
    let durations = [100, 200, 300, 400, 500, 600, 700, 800, 900, 1000];
    let jsonl: String = durations
        .iter()
        .map(|ms| task_line("cli", *ms, &[], "ollama", "gemma4:12b", None, None) + "\n")
        .collect();
    let tasks = parse_events(&jsonl);

    let report = build_report(&tasks, &HashMap::new());

    assert_eq!(report.duration_p50_ms, 500);
    assert_eq!(report.duration_p95_ms, 1000);
}
