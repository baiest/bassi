use std::path::PathBuf;
use std::time::Duration;

use crate::speech::SpeechError;

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:4123";
const DEFAULT_VOICE: &str = "nala";
const DEFAULT_REFERENCE: &str = "data/voices/nala/reference.wav";
const DEFAULT_LANGUAGE: &str = "es";
const DEFAULT_EXAGGERATION: f32 = 0.5;
const DEFAULT_CFG_WEIGHT: f32 = 0.5;
const DEFAULT_TEMPERATURE: f32 = 0.8;
const DEFAULT_TIMEOUT_S: u64 = 30;
const DEFAULT_READ_TIMEOUT_S: u64 = 60;
const DEFAULT_STREAMING_STRATEGY: &str = "sentence";
const DEFAULT_STREAMING_CHUNK_SIZE: u64 = 200;
const DEFAULT_AUTOSTART: bool = true;
// `Command::new` runs this directly (no shell), so a bare `.ps1` path
// wouldn't launch - it has to go through `powershell.exe` explicitly.
const DEFAULT_CMD: &str = "powershell -ExecutionPolicy Bypass -File scripts/chatterbox-server.ps1";
const DEFAULT_STARTUP_TIMEOUT_S: u64 = 180;

/// Configuration for the Chatterbox TTS backend, resolved from environment
/// variables with the same `from_env` + `Default` pattern as `LoopLimits`
/// and `ContextBudget`. Any var that's unset or doesn't parse keeps its
/// default; `reference_path` is validated to exist because a missing voice
/// reference should fail loudly at startup, not silently at first `say`.
#[derive(Debug, Clone)]
pub struct ChatterboxConfig {
    pub base_url: String,
    pub voice: String,
    pub reference_path: PathBuf,
    pub language: String,
    pub exaggeration: f32,
    pub cfg_weight: f32,
    pub temperature: f32,
    pub timeout: Duration,
    /// Per-request timeout covering the whole streamed response, from the
    /// initial connection through the last chunk. Kept separate from
    /// `timeout` (also used as the connect/build timeout) because a
    /// streamed answer legitimately takes much longer than a single
    /// request-response round trip.
    pub read_timeout: Duration,
    /// How the server should chunk text for streaming (`sentence`,
    /// `paragraph`, `fixed`, or `word` - see the server's streaming docs).
    pub streaming_strategy: String,
    /// Target characters per streaming chunk, passed straight through to
    /// the server (accepts 50-500).
    pub streaming_chunk_size: u32,
    pub autostart: bool,
    pub command: String,
    pub startup_timeout: Duration,
}

impl ChatterboxConfig {
    /// Reads `NALA_CHATTERBOX_*` environment variables, falling back to
    /// defaults, and validates that `reference_path` exists on disk.
    pub fn from_env() -> Result<Self, SpeechError> {
        let reference_path = PathBuf::from(
            std::env::var("NALA_CHATTERBOX_REFERENCE")
                .unwrap_or_else(|_| DEFAULT_REFERENCE.to_string()),
        );

        if !reference_path.exists() {
            return Err(SpeechError::Configuration(format!(
                "reference.wav not found at '{}' (set NALA_CHATTERBOX_REFERENCE to override)",
                reference_path.display()
            )));
        }

        Ok(Self {
            base_url: env_string("NALA_CHATTERBOX_URL", DEFAULT_BASE_URL),
            voice: env_string("NALA_CHATTERBOX_VOICE", DEFAULT_VOICE),
            reference_path,
            language: env_string("NALA_CHATTERBOX_LANGUAGE", DEFAULT_LANGUAGE),
            exaggeration: env_f32("NALA_CHATTERBOX_EXAGGERATION", DEFAULT_EXAGGERATION),
            cfg_weight: env_f32("NALA_CHATTERBOX_CFG_WEIGHT", DEFAULT_CFG_WEIGHT),
            temperature: env_f32("NALA_CHATTERBOX_TEMPERATURE", DEFAULT_TEMPERATURE),
            timeout: Duration::from_secs(env_u64("NALA_CHATTERBOX_TIMEOUT_S", DEFAULT_TIMEOUT_S)),
            read_timeout: Duration::from_secs(env_u64(
                "NALA_CHATTERBOX_READ_TIMEOUT_S",
                DEFAULT_READ_TIMEOUT_S,
            )),
            streaming_strategy: env_string(
                "NALA_CHATTERBOX_STREAMING_STRATEGY",
                DEFAULT_STREAMING_STRATEGY,
            ),
            streaming_chunk_size: env_u64(
                "NALA_CHATTERBOX_CHUNK_SIZE",
                DEFAULT_STREAMING_CHUNK_SIZE,
            ) as u32,
            autostart: env_bool("NALA_CHATTERBOX_AUTOSTART", DEFAULT_AUTOSTART),
            command: env_string("NALA_CHATTERBOX_CMD", DEFAULT_CMD),
            startup_timeout: Duration::from_secs(env_u64(
                "NALA_CHATTERBOX_STARTUP_TIMEOUT_S",
                DEFAULT_STARTUP_TIMEOUT_S,
            )),
        })
    }
}

fn env_string(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_f32(key: &str, default: f32) -> f32 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_bool(key: &str, default: bool) -> bool {
    match std::env::var(key) {
        Ok(value) => value != "0",
        Err(_) => default,
    }
}
