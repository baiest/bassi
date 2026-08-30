use std::path::PathBuf;

use nala::adapters::events::console::ConsoleEventSink;
use nala::adapters::metrics::csv_sink::CsvMetricsSink;
use nala::bootstrap::{self, DEFAULT_MODEL};
use nala::cli::prompt::MultilineReader;

fn main() {
    let events = ConsoleEventSink;
    // Off by default (NALA_METRICS_DIR unset) so development runs and tests
    // don't scatter CSV files on disk; set it to opt into per-task token
    // accounting for later cost estimation.
    let metrics_dir = std::env::var("NALA_METRICS_DIR").ok().map(PathBuf::from);
    let model = std::env::var("NALA_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());
    let events = CsvMetricsSink::new(events, metrics_dir, "ollama", &model);

    let assistant = bootstrap::build_assistant(events);

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
