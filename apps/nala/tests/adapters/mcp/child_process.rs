use std::time::{Duration, Instant};

use nala::adapters::mcp::child_process::ChildTransport;
use nala::adapters::mcp::stdio::Transport;

/// Exercises the real adapter against a real OS process — like
/// `tests/adapters/process/windows.rs`, this is a real child process rather
/// than the `FakeTransport` used for `StdioMcpClient`'s protocol tests.
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
