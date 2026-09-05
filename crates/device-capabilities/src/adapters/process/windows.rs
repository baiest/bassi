use std::io::Read;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::ports::process::{Process, ProcessError};

/// How often the deadline loop polls the child's exit status while waiting.
/// Short enough that a quick command isn't held up noticeably past its real
/// completion, long enough not to spin the CPU.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Extra time, past the child's own deadline, `spawn` gives the reader
/// threads to hand over whatever they already read. Without it, a command
/// that finishes right at its deadline would have zero budget left to
/// deliver output it already wrote before `recv_timeout` gives up. Small
/// because it only covers a race at the boundary, not the actual read.
const READER_DRAIN_GRACE: Duration = Duration::from_millis(250);

pub struct Windows;

impl Windows {
    pub fn new() -> Self {
        Self
    }

    /// Shared body for `spawn`/`spawn_detached`. `capture` selects whether
    /// stdout/stderr are piped and read back (`spawn`) or sent to
    /// `Stdio::null()` with no reader threads at all (`spawn_detached`) --
    /// the latter exists so a fire-and-forget launcher never opens a pipe a
    /// GUI grandchild could inherit and hold open forever. See BAS-61.
    fn run(
        &mut self,
        command: &str,
        args: &[&str],
        timeout: Duration,
        capture: bool,
    ) -> Result<String, ProcessError> {
        if command.trim().is_empty() {
            return Err(ProcessError::InvalidArguments(
                "Command cannot be empty".to_string(),
            ));
        }

        // `raw_arg`, not `.arg`/`.args`: every caller here passes `/C` plus
        // an already-fully-quoted command string (e.g. `start "" "<url>"`)
        // for cmd.exe's own quirky `/C` parser to interpret. `.arg` would
        // re-escape an argument that already contains quotes using Rust's
        // CreateProcess quoting rules, producing a Win32 command line
        // cmd.exe doesn't parse the way a human typing it would expect --
        // see BAS-60. `raw_arg` inserts each argument into the command
        // line exactly as given, with no extra quoting.
        let mut cmd = Command::new(command);
        for arg in args {
            cmd.raw_arg(arg);
        }

        // Piped rather than `.output()`'s blocking wait, so a command that
        // runs past `timeout` can be killed instead of hanging the turn
        // forever (e.g. a GUI app that never returns, or a command that
        // waits on input nala never provides). The detached path uses
        // `Stdio::null()` instead: nothing reads it back, so there is no
        // pipe for a grandchild to inherit and hold open -- see BAS-61.
        let stdio = || {
            if capture {
                Stdio::piped()
            } else {
                Stdio::null()
            }
        };
        let mut child = cmd
            .stdout(stdio())
            .stderr(stdio())
            .spawn()
            .map_err(|error| ProcessError::ProcessFailed(error.to_string()))?;

        // Assigned to a job so that if the command itself spawns further
        // processes (e.g. `start chrome` handing off to chrome.exe), a
        // timeout kills the whole tree, not just the immediate child. Same
        // rationale as `adapters/mcp/child_process.rs`.
        #[cfg(windows)]
        let job = process_group::ProcessGroup::new().ok();
        #[cfg(windows)]
        if let Some(job) = &job {
            let _ = job.assign(&child);
        }

        // Read concurrently, not after `wait()`: a child that writes more
        // than the OS pipe buffer holds would otherwise block forever with
        // nobody draining it, deadlocking against our own poll loop below.
        // Each reader hands its buffer over a channel instead of being
        // joined directly -- see the `recv_timeout` below for why. Skipped
        // entirely when `!capture`: there is no pipe to read.
        let (stdout_tx, stdout_rx) = mpsc::channel();
        let (stderr_tx, stderr_rx) = mpsc::channel();
        if capture {
            let mut stdout_pipe = child.stdout.take();
            let mut stderr_pipe = child.stderr.take();
            std::thread::spawn(move || {
                let mut buffer = Vec::new();
                if let Some(stdout) = stdout_pipe.as_mut() {
                    let _ = stdout.read_to_end(&mut buffer);
                }
                let _ = stdout_tx.send(buffer);
            });
            std::thread::spawn(move || {
                let mut buffer = Vec::new();
                if let Some(stderr) = stderr_pipe.as_mut() {
                    let _ = stderr.read_to_end(&mut buffer);
                }
                let _ = stderr_tx.send(buffer);
            });
        }

        let deadline = Instant::now() + timeout;
        let status = loop {
            match child
                .try_wait()
                .map_err(|error| ProcessError::ProcessFailed(error.to_string()))?
            {
                Some(status) => break status,
                None => {
                    if Instant::now() >= deadline {
                        #[cfg(windows)]
                        drop(job);
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(ProcessError::Timeout(timeout));
                    }
                    std::thread::sleep(POLL_INTERVAL);
                }
            }
        };

        // A grandchild that inherited a pipe's write handle (e.g. `start ""
        // "notepad"` -- notepad.exe outlives cmd.exe and keeps the handle)
        // means `read_to_end` never sees EOF, so joining the reader thread
        // directly can block forever even though the command itself already
        // finished. `recv_timeout`, bounded by whatever remains of the
        // deadline (plus a small grace, see `READER_DRAIN_GRACE`), makes
        // sure `spawn` never blocks past `timeout` either way -- if the
        // channel doesn't answer in time, its thread is simply abandoned (a
        // deliberate leak, same trade-off as `crates/mcp/child_process.rs`):
        // an inherited handle can wedge that thread forever, but it never
        // wedges the caller. See BAS-61. Skipped when `!capture`: there was
        // never a reader thread to wait on.
        let (stdout, stderr) = if capture {
            let grace = deadline
                .saturating_duration_since(Instant::now())
                .max(READER_DRAIN_GRACE);
            let stdout = stdout_rx.recv_timeout(grace).unwrap_or_default();
            let grace = deadline
                .saturating_duration_since(Instant::now())
                .max(READER_DRAIN_GRACE);
            let stderr = stderr_rx.recv_timeout(grace).unwrap_or_default();
            (stdout, stderr)
        } else {
            (Vec::new(), Vec::new())
        };

        // The job is only meant to kill stragglers on a timeout (handled
        // above, before this point). On a normal return, dropping `job`
        // would close its handle and — because of
        // `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` — kill everything still
        // assigned to it, including a GUI app the command just launched
        // (e.g. `start app.exe`) that is still running when `cmd.exe`
        // itself has already exited. Forget it instead so the launched
        // process(es) survive; the handle leaks for the life of this
        // process, which is an acceptable trade for not killing what the
        // user asked to open.
        #[cfg(windows)]
        std::mem::forget(job);

        if !status.success() {
            return Err(ProcessError::ProcessFailed(if capture {
                format!(
                    "Command failed: {command} {:?}\n{}",
                    args,
                    String::from_utf8_lossy(&stderr)
                )
            } else {
                format!("Command failed: {command} {args:?} (no output captured)")
            }));
        }

        String::from_utf8(stdout).map_err(|error| ProcessError::ProcessFailed(error.to_string()))
    }
}

impl Default for Windows {
    fn default() -> Self {
        Self::new()
    }
}

impl Process for Windows {
    const SYSTEM_DESCRIPTION: &'static str =
        "Commands are executed using Windows cmd.exe EXCLUSIVE. IMPORTANT Use Windows cmd syntax.";

    fn spawn(
        &mut self,
        command: &str,
        args: &[&str],
        timeout: Duration,
    ) -> Result<String, ProcessError> {
        self.run(command, args, timeout, true)
    }

    fn spawn_detached(
        &mut self,
        command: &str,
        args: &[&str],
        timeout: Duration,
    ) -> Result<(), ProcessError> {
        self.run(command, args, timeout, false).map(|_| ())
    }
}
