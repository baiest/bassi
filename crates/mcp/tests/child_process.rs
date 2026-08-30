#![cfg(windows)]

use std::time::{Duration, Instant};

use mcp::{ChildTransport, Transport};

/// Exercises the real adapter against a real OS process, not the
/// `FakeTransport` used for `StdioMcpClient`'s protocol tests. Windows
/// only: the command spawned below relies on `cmd /C` redirection, which
/// only `ChildTransport` sets up on Windows.
#[test]
fn read_line_times_out_instead_of_hanging_when_the_server_never_responds() {
    // A process that stays alive without ever writing a line to stdout —
    // the situation a wedged MCP server would put us in.
    let mut transport = ChildTransport::spawn(
        "ping",
        &["-n", "30", "127.0.0.1", ">NUL"],
        Duration::from_millis(300),
    )
    .expect("failed to spawn ping");

    let start = Instant::now();
    let result = transport.read_line();
    let elapsed = start.elapsed();

    assert!(result.is_err(), "expected a timeout, got {result:?}");
    assert!(
        elapsed < Duration::from_secs(5),
        "expected read_line to give up promptly, took {elapsed:?}"
    );
}
