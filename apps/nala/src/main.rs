use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use nala::adapters::events::console::ConsoleEventSink;
use nala::adapters::metrics::csv_sink::CsvMetricsSink;
use nala::adapters::metrics::jsonl_sink::JsonlMetricsSink;
use nala::application::devices::registry::DeviceRegistry;
use nala::application::metrics_report::{PricingTable, build_report, parse_events};
use nala::bootstrap;
use nala::cli::prompt::MultilineReader;

/// Default pricing table path, overridable with `NALA_PRICING_FILE`.
const DEFAULT_PRICING_FILE: &str = "config/pricing.json";

/// Default bind address for `nala --serve`, overridable with `NALA_ADDR`.
/// Loopback-only by default: the server has no auth, and Nala's native
/// tools (`open_app`, `volume`, `open_url`, `execute_command`) shouldn't be
/// reachable from the LAN until that changes.
const DEFAULT_ADDR: &str = "127.0.0.1:4180";

/// Default bind address for the device listener, overridable with
/// `NALA_DEVICE_ADDR`. Loopback-only for the same reason as `DEFAULT_ADDR`
/// — a connected device can run `execute_command`.
const DEFAULT_DEVICE_ADDR: &str = "127.0.0.1:4182";

fn main() {
    if std::env::args().nth(1).as_deref() == Some("metrics") {
        if std::env::args().nth(2).as_deref() != Some("report") {
            eprintln!("Usage: nala metrics report [--json]");
            std::process::exit(1);
        }
        run_metrics_report(std::env::args().any(|arg| arg == "--json"));
        return;
    }

    if std::env::args().nth(1).as_deref() == Some("--serve") {
        let addr = std::env::var("NALA_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());
        let device_addr =
            std::env::var("NALA_DEVICE_ADDR").unwrap_or_else(|_| DEFAULT_DEVICE_ADDR.to_string());
        // `None` (the env var unset) means every device connection is
        // rejected — see `device_server::validate_hello` — rather than
        // silently accepting devices with no authentication at all.
        let device_token = std::env::var("NALA_DEVICE_TOKEN").ok();

        // Same default as the local REPL below, so every served turn also
        // gets token/duration accounting instead of only CLI sessions.
        let metrics_dir = Some(
            std::env::var("NALA_METRICS_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("data/metrics")),
        );

        let devices = Arc::new(DeviceRegistry::new());

        let device_server_devices = Arc::clone(&devices);
        thread::spawn(move || {
            if let Err(error) =
                nala::device_server::serve(&device_addr, device_server_devices, device_token)
            {
                eprintln!("Error: could not start the device server on {device_addr}: {error}");
            }
        });

        if let Err(error) = nala::server::serve(&addr, devices, metrics_dir) {
            eprintln!("Error: could not start the server on {addr}: {error}");
            std::process::exit(1);
        }
        return;
    }

    let events = ConsoleEventSink;
    // Defaults to data/metrics so every run gets token accounting without
    // extra setup; override with NALA_METRICS_DIR to point elsewhere.
    let metrics_dir = Some(
        std::env::var("NALA_METRICS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("data/metrics")),
    );
    let events = JsonlMetricsSink::new(events, metrics_dir.clone());
    let events = CsvMetricsSink::new(events, metrics_dir);

    // The local REPL has no device server running, so no device is ever
    // connected — an empty registry, not a special case in `build_assistant`.
    let devices = Arc::new(DeviceRegistry::new());
    let assistant = bootstrap::build_assistant(events, devices);

    // Ctrl+C during a turn (not at the prompt, where reedline already
    // handles it) cancels the turn instead of killing the process.
    let (mut assistant, cancel_signal) = bootstrap::install_cancel_signal(assistant);

    let mut reader = MultilineReader::new();

    println!("Nala is ready. What can I help with?");

    loop {
        println!(
            "(you can write/paste multiple lines and use arrows/backspace between them; Ctrl+Enter submits)"
        );

        let input = match reader.read().expect("Failed reading input") {
            Some(input) => input,
            None => break,
        };

        #[cfg(windows)]
        if let Some(signal) = &cancel_signal {
            signal.reset();
        }
        #[cfg(not(windows))]
        let _ = &cancel_signal;

        match assistant.process(input.trim()) {
            Ok(response) => println!("{response}"),
            Err(e) => eprintln!("Error: {e}"),
        }
    }
}

/// Reads `events.jsonl` and the pricing table, builds the report (pure
/// logic lives in `application::metrics_report`), and prints it — as JSON
/// with `--json`, otherwise a human-readable summary.
fn run_metrics_report(as_json: bool) {
    let metrics_dir = std::env::var("NALA_METRICS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("data/metrics"));
    let events_path = metrics_dir.join("events.jsonl");
    let jsonl = std::fs::read_to_string(&events_path).unwrap_or_else(|error| {
        eprintln!(
            "Error: could not read {} ({error}) -- run Nala at least once first",
            events_path.display()
        );
        std::process::exit(1);
    });

    let pricing_path = std::env::var("NALA_PRICING_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_PRICING_FILE));
    let pricing: PricingTable = std::fs::read_to_string(&pricing_path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| {
            eprintln!(
                "Warning: could not read/parse {} -- cost estimates will be empty",
                pricing_path.display()
            );
            PricingTable::new()
        });

    let tasks = parse_events(&jsonl);
    let report = build_report(&tasks, &pricing);

    if as_json {
        println!(
            "{}",
            serde_json::json!({
                "total_tasks": report.total_tasks,
                "total_input_tokens": report.total_input_tokens,
                "total_output_tokens": report.total_output_tokens,
                "duration_p50_ms": report.duration_p50_ms,
                "duration_p95_ms": report.duration_p95_ms,
                "by_source": report.by_source,
                "tool_usage": report.tool_usage,
                "tool_errors": report.tool_errors,
                "estimated_cost_usd": report.estimated_cost_usd,
            })
        );
        return;
    }

    println!("Nala metrics report ({} tasks)", report.total_tasks);
    println!(
        "  tokens: {} in / {} out",
        report.total_input_tokens, report.total_output_tokens
    );
    println!(
        "  duration: p50 {}ms, p95 {}ms",
        report.duration_p50_ms, report.duration_p95_ms
    );

    println!("\nBy source:");
    for (source, count) in sorted_by_count(&report.by_source) {
        println!("  {source}: {count}");
    }

    println!("\nTop tools:");
    for (tool, count) in sorted_by_count(&report.tool_usage) {
        let errors = report.tool_errors.get(&tool).copied().unwrap_or(0);
        println!("  {tool}: {count} calls, {errors} errors");
    }

    println!(
        "\nEstimated cost had this token volume run through each provider \
         (NOT a real bill -- token counts come from Ollama's tokenizer, \
         not the target provider's):"
    );
    let mut costs: Vec<(&String, &f64)> = report.estimated_cost_usd.iter().collect();
    costs.sort_by(|a, b| a.0.cmp(b.0));
    for (key, cost) in costs {
        println!("  {key}: ${cost:.4}");
    }
}

fn sorted_by_count(counts: &std::collections::HashMap<String, usize>) -> Vec<(String, usize)> {
    let mut entries: Vec<(String, usize)> = counts
        .iter()
        .map(|(key, count)| (key.clone(), *count))
        .collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    entries
}
