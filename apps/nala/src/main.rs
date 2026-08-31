use std::path::PathBuf;

use nala::adapters::events::console::ConsoleEventSink;
use nala::adapters::metrics::csv_sink::CsvMetricsSink;
use nala::bootstrap::{self, DEFAULT_MODEL};
use nala::cli::prompt::MultilineReader;

/// Default bind address for `nala --serve`, overridable with `NALA_ADDR`.
/// Loopback-only by default: the server has no auth, and Nala's native
/// tools (`open_app`, `volume`, `open_url`, `execute_command`) shouldn't be
/// reachable from the LAN until that changes.
const DEFAULT_ADDR: &str = "127.0.0.1:4180";

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--serve") {
        let addr = std::env::var("NALA_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());
        if let Err(error) = nala::server::serve(&addr) {
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
