#[path = "common/fake_process.rs"]
mod fake_process;

#[path = "common/fake_environment.rs"]
mod fake_environment;

#[path = "adapters/computer/windows.rs"]
mod computer;

// Spawns a real `cmd`/`ping` process — Windows-only, like the adapter it
// tests.
#[cfg(windows)]
#[path = "adapters/process/windows.rs"]
mod process_windows;
