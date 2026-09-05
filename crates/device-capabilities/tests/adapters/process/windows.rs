use std::time::Duration;

use device_capabilities::adapters::process::windows::Windows;
use device_capabilities::ports::process::{Process, ProcessError};

/// Exercises the real adapter against a real OS process — unlike the rest
/// of the suite, which uses `FakeProcess`. Kept minimal since a real child
/// process is inherently slower and more environment-dependent than an
/// in-memory fake.
#[test]
fn returns_the_command_output() {
    let mut process = Windows::new();

    let result = process.spawn("cmd", &["/C", "echo hello"], Duration::from_secs(5));

    assert_eq!(result.unwrap().trim(), "hello");
}

#[test]
fn an_argument_containing_its_own_quotes_reaches_cmd_unmangled() {
    // `Command::new("cmd").args(["/C", command])` re-escapes `command` with
    // Rust's own CreateProcess quoting rules even when it already contains
    // its own embedded quotes (e.g. `start "" "<url>"`) -- producing a
    // Win32 command line cmd.exe's own /C parser doesn't interpret the way
    // a human typing it would expect. `echo` makes this reproducible
    // deterministically, without a GUI: it should print exactly what
    // follows `/C`, quotes included and no extra backslashes. See BAS-60.
    let mut process = Windows::new();

    let result = process.spawn(
        "cmd",
        &["/C", "echo \"\" \"hello\""],
        Duration::from_secs(5),
    );

    assert_eq!(result.unwrap().trim(), "\"\" \"hello\"");
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

/// Reproduces the real hang seen with `start "" "<app or url>"`: `cmd.exe`
/// itself exits almost instantly, but a grandchild process that inherits
/// its stdout/stderr write handles keeps the pipe open, so
/// `read_to_end` on the read end never sees EOF. Before this fix, `spawn`'s
/// unbounded `stdout_reader.join()`/`stderr_reader.join()` blocked past the
/// `timeout` given here -- observed live blocking pc-daemon's single-
/// threaded session loop for 100+ seconds. See BAS-61.
///
/// `/B` on the inner `start` is load-bearing: without it, `start` gives the
/// grandchild its own console and fresh handles, the pipe closes normally,
/// and the bug does not reproduce. With `/B` the grandchild runs in this
/// same console and inherits our stdout/stderr write handles, exactly like
/// `start "" "notepad"` does.
#[test]
fn returns_even_when_a_grandchild_still_holds_the_pipe() {
    let mut process = Windows::new();

    let start = std::time::Instant::now();
    let _ = process.spawn(
        "cmd",
        &[
            "/C",
            "start \"\" /B cmd /C ping -n 20 127.0.0.1 >NUL",
        ],
        Duration::from_secs(1),
    );
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(6),
        "expected spawn to return promptly even with a grandchild holding \
         the pipe open, took {elapsed:?}"
    );
}
