use std::time::Duration;

use device_capabilities::adapters::process::windows::Windows;
use device_capabilities::ports::process::{Process, ProcessError};

/// Exercises the real adapter against a real OS process — unlike the rest
/// of the suite, which uses `FakeProcess`. Kept minimal (two cases) since a
/// real child process is inherently slower and more environment-dependent
/// than an in-memory fake.
#[test]
fn returns_the_command_output() {
    let mut process = Windows::new();

    let result = process.spawn("cmd", &["/C", "echo hello"], Duration::from_secs(5));

    assert_eq!(result.unwrap().trim(), "hello");
}

#[test]
fn kills_a_command_that_runs_past_the_timeout() {
    let mut process = Windows::new();

    let start = std::time::Instant::now();
    let result = process.spawn(
        "cmd",
        &["/C", "ping -n 30 127.0.0.1 >NUL"],
        Duration::from_millis(300),
    );
    let elapsed = start.elapsed();

    assert!(matches!(result, Err(ProcessError::Timeout(_))));
    assert!(
        elapsed < Duration::from_secs(5),
        "expected the process to be killed promptly, took {elapsed:?}"
    );
}
