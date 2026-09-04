use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use nala::adapters::events::console::ConsoleEventSink;
use nala::adapters::metrics::csv_sink::CsvMetricsSink;
use nala::application::devices::registry::DeviceRegistry;
use nala::bootstrap;
use nala::cli::prompt::MultilineReader;

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
