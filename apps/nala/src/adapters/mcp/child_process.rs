use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use crate::adapters::mcp::stdio::Transport;

/// A `Transport` backed by a long-lived child process's stdin/stdout,
/// speaking one JSON-RPC message per line. Used to talk to MCP servers
/// started with `npx`.
///
/// Not covered by the automated test suite (like
/// `adapters/process/windows.rs`): it depends on a real OS process and an
/// installed `npx`. `StdioMcpClient`'s protocol logic is covered against an
/// in-memory `Transport` fake instead; see `tests/adapters/mcp/stdio.rs`.
pub struct ChildTransport {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl ChildTransport {
    pub fn spawn(program: &str, args: &[&str]) -> std::io::Result<Self> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other("child process has no stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("child process has no stdout"))?;

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }
}

impl Transport for ChildTransport {
    fn send_line(&mut self, line: &str) -> std::io::Result<()> {
        writeln!(self.stdin, "{line}")?;
        self.stdin.flush()
    }

    fn read_line(&mut self) -> std::io::Result<String> {
        loop {
            let mut line = String::new();
            let bytes_read = self.stdout.read_line(&mut line)?;

            if bytes_read == 0 {
                return Err(std::io::Error::other("child process closed stdout"));
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            return Ok(trimmed.to_string());
        }
    }
}

impl Drop for ChildTransport {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
