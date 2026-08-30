use std::process::{Child, Command};
use std::time::{Duration, Instant};

use crate::speech::SpeechError;

use super::config::ChatterboxConfig;

#[cfg(windows)]
use process_group::ProcessGroup;

/// What to do about the Chatterbox server, given whether it already
/// answers `/health` and whether autostart is allowed. Pulled out as a
/// pure function so this decision is testable without spawning a real
/// process or opening a real socket.
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    /// A server already answered `/health`; nothing to spawn.
    AlreadyRunning,
    /// No server answered and autostart is allowed; spawn the server.
    Spawn,
    /// No server answered and autostart is disabled.
    AutostartDisabled,
}

pub fn decide(is_healthy: bool, autostart: bool) -> Decision {
    match (is_healthy, autostart) {
        (true, _) => Decision::AlreadyRunning,
        (false, true) => Decision::Spawn,
        (false, false) => Decision::AutostartDisabled,
    }
}

/// Owns the Chatterbox server process this instance started, if any, and
/// kills it on drop. If a server was already running instead, no child is
/// owned and dropping this does nothing to it - that server is left for
/// whoever started it.
pub struct ChatterboxSupervisor {
    child: Option<Child>,
    #[cfg(windows)]
    _job: Option<ProcessGroup>,
}

impl ChatterboxSupervisor {
    /// Ensures a Chatterbox server is reachable at `config.base_url`,
    /// starting one via `config.command` if needed and allowed. Requires a
    /// live Chatterbox install/venv to succeed, so this is exercised by
    /// manual verification and `--ignored` tests, not the default
    /// `cargo test` run.
    pub fn ensure_running(config: &ChatterboxConfig) -> Result<Self, SpeechError> {
        if health_check(&config.base_url) {
            return Ok(Self::not_owned());
        }

        match decide(false, config.autostart) {
            Decision::AutostartDisabled => Err(SpeechError::Unavailable(format!(
                "Chatterbox not reachable at {} and NALA_CHATTERBOX_AUTOSTART=0",
                config.base_url
            ))),
            Decision::Spawn => Self::spawn_and_wait(config),
            Decision::AlreadyRunning => unreachable!("health_check just returned false"),
        }
    }

    fn not_owned() -> Self {
        Self {
            child: None,
            #[cfg(windows)]
            _job: None,
        }
    }

    fn spawn_and_wait(config: &ChatterboxConfig) -> Result<Self, SpeechError> {
        let mut parts = config.command.split_whitespace();
        let program = parts.next().ok_or_else(|| {
            SpeechError::Configuration("NALA_CHATTERBOX_CMD is empty".to_string())
        })?;

        let mut child = Command::new(program).args(parts).spawn().map_err(|error| {
            SpeechError::Unavailable(format!("failed to start Chatterbox server: {error}"))
        })?;

        #[cfg(windows)]
        let job = match Self::assign_job(&child) {
            Ok(job) => Some(job),
            Err(error) => {
                let _ = child.kill();
                return Err(error);
            }
        };

        let deadline = Instant::now() + config.startup_timeout;
        while Instant::now() < deadline {
            if health_check(&config.base_url) {
                return Ok(Self {
                    child: Some(child),
                    #[cfg(windows)]
                    _job: job,
                });
            }
            std::thread::sleep(Duration::from_millis(500));
        }

        let _ = child.kill();
        Err(SpeechError::Unavailable(format!(
            "Chatterbox server did not become healthy within {:?}",
            config.startup_timeout
        )))
    }

    #[cfg(windows)]
    fn assign_job(child: &Child) -> Result<ProcessGroup, SpeechError> {
        let job = ProcessGroup::new().map_err(|error| {
            SpeechError::Unavailable(format!("failed to create process group: {error}"))
        })?;
        job.assign(child).map_err(|error| {
            SpeechError::Unavailable(format!("failed to assign process group: {error}"))
        })?;
        Ok(job)
    }
}

impl Drop for ChatterboxSupervisor {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
        }
    }
}

fn health_check(base_url: &str) -> bool {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .ok()
        .and_then(|client| client.get(format!("{base_url}/health")).send().ok())
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}
