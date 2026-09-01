// Hides the console window on Windows, so the daemon runs in the
// background instead of leaving a terminal open — required for it to
// survive the user closing that terminal, and for autostart to not pop one
// up at login. `println!`/`eprintln!` still compile but have nowhere to
// go once there's no console; proper logging is future work, not needed
// for this vertical slice.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::env;
use std::sync::Arc;
use std::thread;

use device_capabilities::adapters::computer::windows::Windows;
use device_capabilities::adapters::environment::system::SystemEnvironment;
use device_capabilities::adapters::process::windows::Windows as WindowsProcess;
use device_capabilities::capabilities::execute_command::ExecuteCommandTool;
use device_capabilities::capabilities::list_apps::ListAppsTool;
use device_capabilities::capabilities::open_app::OpenAppTool;
use device_capabilities::capabilities::open_url::OpenUrlTool;
use device_capabilities::capabilities::volume::VolumeTool;
use device_capabilities::registry::CapabilityRegistry;
use pc_daemon::client::TcpDeviceWire;
use pc_daemon::config::DeviceIdentity;
use pc_daemon::daemon::{SessionOutcome, run_session};
use pc_daemon::overlay_channel::{self, OverlayChannel};
use pc_daemon::reconnect::{Backoff, RECONNECT_INITIAL_DELAY, RECONNECT_MAX_DELAY};
use tts::WindowsSapiSpeech;

/// Loopback-only by default, matching `nala`'s own `DEFAULT_ADDR` in
/// `main.rs`: exposing this to the LAN is an explicit opt-in, not the
/// default, since a connected daemon can run `execute_command`.
const DEFAULT_ADDR: &str = "127.0.0.1:4182";

/// Loopback-only, and not overridable — the overlay channel has no auth of
/// its own, so it must never leave the machine.
const OVERLAY_ADDR: &str = "127.0.0.1:4183";

/// `execute_command` is left out of the default allowlist deliberately —
/// it's arbitrary command execution, and should be opted into via
/// `NALA_DEVICE_CAPABILITIES` rather than enabled by default.
const DEFAULT_CAPABILITIES: &str = "open_app,open_url,volume,list_apps";

fn new_computer() -> Windows<WindowsProcess, SystemEnvironment> {
    Windows::new(WindowsProcess::new(), SystemEnvironment::new())
}

fn build_registry() -> CapabilityRegistry {
    let allowed =
        env::var("NALA_DEVICE_CAPABILITIES").unwrap_or_else(|_| DEFAULT_CAPABILITIES.to_string());
    let names: Vec<String> = allowed
        .split(',')
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect();

    let mut registry = CapabilityRegistry::with_allowlist(names);

    registry.register(ExecuteCommandTool::new(new_computer()));
    registry.register(OpenAppTool::new(new_computer()));
    registry.register(OpenUrlTool::new(new_computer()));
    registry.register(VolumeTool::new(new_computer()));
    registry.register(ListAppsTool::new(new_computer()));

    registry
}

fn main() {
    let addr = env::var("NALA_DEVICE_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_string());
    let identity = DeviceIdentity {
        device_id: env::var("NALA_DEVICE_ID").unwrap_or_else(|_| "pc".to_string()),
        name: env::var("NALA_DEVICE_NAME").unwrap_or_else(|_| "pc".to_string()),
        platform: "windows".to_string(),
        token: env::var("NALA_DEVICE_TOKEN").unwrap_or_default(),
    };

    let overlay = Arc::new(OverlayChannel::new());
    let overlay_for_server = Arc::clone(&overlay);
    thread::spawn(move || {
        if let Err(error) = overlay_channel::serve(OVERLAY_ADDR, overlay_for_server) {
            eprintln!("Warning: could not start the overlay channel on {OVERLAY_ADDR}: {error}");
        }
    });

    let speech = WindowsSapiSpeech::new();
    let mut backoff = Backoff::new(RECONNECT_INITIAL_DELAY, RECONNECT_MAX_DELAY);

    loop {
        match TcpDeviceWire::connect(&addr) {
            Ok(mut wire) => {
                backoff.reset();
                println!("Connected to nala at {addr}.");

                let mut registry = build_registry();
                match run_session(&mut wire, &mut registry, &identity, &overlay, &speech) {
                    Ok(SessionOutcome::Closed) => {
                        eprintln!("Connection to nala at {addr} closed; reconnecting.");
                    }
                    Ok(SessionOutcome::Rejected(reason)) => {
                        eprintln!("Nala rejected this daemon ({reason:?}); not retrying.");
                        return;
                    }
                    Err(error) => {
                        eprintln!("Warning: connection error: {error}");
                    }
                }
            }
            Err(error) => {
                eprintln!("Warning: could not connect to nala at {addr}: {error}");
            }
        }

        thread::sleep(backoff.next_delay());
    }
}
