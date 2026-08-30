use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use crate::stdio::Transport;

/// A `Transport` backed by a long-lived child process's stdin/stdout,
/// speaking one JSON-RPC message per line. Used to talk to MCP servers
/// started with `npx` or any other launcher.
///
/// Not covered by the automated test suite: it depends on a real OS process
/// and an installed launcher. `StdioMcpClient`'s protocol logic is covered
/// against an in-memory `Transport` fake instead; see `tests/stdio.rs`.
pub struct ChildTransport {
    child: Child,
    stdin: ChildStdin,
    /// Lines read by the background reader thread below, one per
    /// completed `read_line` on the child's stdout. A plain synchronous
    /// read has no way to time out mid-call, so the actual reading happens
    /// on its own thread and `read_line` just waits on this channel with a
    /// deadline — an unresponsive MCP server blocks that thread forever,
    /// but never blocks the caller past `timeout`.
    lines: mpsc::Receiver<std::io::Result<String>>,
    timeout: Duration,
    /// Kept alive only to be dropped (and so kill the whole process group)
    /// after `child` on `ChildTransport::drop`. See `process_group`'s docs
    /// for why `child.kill()` alone isn't enough on Windows.
    #[cfg(windows)]
    _job: Option<process_group::ProcessGroup>,
}

impl ChildTransport {
    pub fn spawn(program: &str, args: &[&str], timeout: Duration) -> std::io::Result<Self> {
        // On Windows, package-manager shims like `npx` are `.cmd` batch
        // files, not real executables — `CreateProcess` (what
        // `std::process::Command` calls under the hood) can't launch those
        // directly, only `cmd.exe` can.
        let mut command = if cfg!(windows) {
            let mut command = Command::new("cmd");
            command.arg("/C").arg(program).args(args);
            command
        } else {
            let mut command = Command::new(program);
            command.args(args);
            command
        };

        // Created before spawning so the child can be assigned to it right
        // away, before it has a chance to spawn its own children.
        #[cfg(windows)]
        let job = process_group::ProcessGroup::new().ok();

        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        #[cfg(windows)]
        if let Some(job) = &job {
            // Best-effort: if this fails, cleanup falls back to killing
            // just the direct child, same as before this existed.
            let _ = job.assign(&child);
        }

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other("child process has no stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("child process has no stdout"))?;

        let (sender, lines) = mpsc::channel();
        std::thread::spawn(move || read_lines(stdout, sender));

        Ok(Self {
            child,
            stdin,
            lines,
            timeout,
            #[cfg(windows)]
            _job: job,
        })
    }
}

/// Runs on its own thread for the lifetime of the child process, forwarding
/// each complete line (skipping blank ones) to `sender`. Exits once the
/// pipe closes, a read fails, or nobody is listening any more.
fn read_lines(stdout: ChildStdout, sender: mpsc::Sender<std::io::Result<String>>) {
    let mut reader = BufReader::new(stdout);

    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line);

        let message = match read {
            Ok(0) => Err(std::io::Error::other("child process closed stdout")),
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                Ok(trimmed.to_string())
            }
            Err(error) => Err(error),
        };

        let is_terminal = message.is_err();
        if sender.send(message).is_err() || is_terminal {
            return;
        }
    }
}

impl Transport for ChildTransport {
    fn send_line(&mut self, line: &str) -> std::io::Result<()> {
        writeln!(self.stdin, "{line}")?;
        self.stdin.flush()
    }

    fn read_line(&mut self) -> std::io::Result<String> {
        match self.lines.recv_timeout(self.timeout) {
            Ok(message) => message,
            Err(mpsc::RecvTimeoutError::Timeout) => Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("no response from MCP server within {:?}", self.timeout),
            )),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(std::io::Error::other("MCP server's reader thread stopped"))
            }
        }
    }
}

impl Drop for ChildTransport {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
