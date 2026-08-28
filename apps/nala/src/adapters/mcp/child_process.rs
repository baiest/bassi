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
    /// Kept alive only to be dropped (and so kill the whole process group)
    /// after `child` on `ChildTransport::drop`. See `job_object`'s docs for
    /// why `child.kill()` alone isn't enough on Windows.
    #[cfg(windows)]
    _job: Option<crate::adapters::mcp::job_object::ProcessGroup>,
}

impl ChildTransport {
    pub fn spawn(program: &str, args: &[&str]) -> std::io::Result<Self> {
        // On Windows, package-manager shims like `npx` are `.cmd` batch
        // files, not real executables — `CreateProcess` (what
        // `std::process::Command` calls under the hood) can't launch those
        // directly, only `cmd.exe` can. Routing through `cmd /C` mirrors
        // what `adapters/computer/windows.rs` already does for shell
        // commands, and still works for a real .exe.
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
        let job = crate::adapters::mcp::job_object::ProcessGroup::new().ok();

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

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            #[cfg(windows)]
            _job: job,
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
