use std::io::Read;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::ports::process::{Process, ProcessError};

/// How often the deadline loop polls the child's exit status while waiting.
/// Short enough that a quick command isn't held up noticeably past its real
/// completion, long enough not to spin the CPU.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

pub struct Windows;

impl Windows {
    pub fn new() -> Self {
        Self
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
        // waits on input nala never provides).
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
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
        let mut stdout_pipe = child.stdout.take();
        let mut stderr_pipe = child.stderr.take();
        let stdout_reader = std::thread::spawn(move || {
            let mut buffer = Vec::new();
            if let Some(stdout) = stdout_pipe.as_mut() {
                let _ = stdout.read_to_end(&mut buffer);
            }
            buffer
        });
        let stderr_reader = std::thread::spawn(move || {
            let mut buffer = Vec::new();
            if let Some(stderr) = stderr_pipe.as_mut() {
                let _ = stderr.read_to_end(&mut buffer);
            }
            buffer
        });

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

        let stdout = stdout_reader.join().unwrap_or_default();
        let stderr = stderr_reader.join().unwrap_or_default();

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
            return Err(ProcessError::ProcessFailed(format!(
                "Command failed: {command} {:?}\n{}",
                args,
                String::from_utf8_lossy(&stderr)
            )));
        }

        String::from_utf8(stdout).map_err(|error| ProcessError::ProcessFailed(error.to_string()))
    }
}
