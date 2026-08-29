use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

use crate::ports::speech::SpeechError;

const DEFAULT_BIN: &str = "tools/piper/piper.exe";
const DEFAULT_MODEL: &str = "data/voices/piper/es_MX-claude-high.onnx";
const DEFAULT_LENGTH_SCALE: f32 = 1.0;
const DEFAULT_NOISE_SCALE: f32 = 0.667;

/// Configuration for the Piper TTS backend, resolved from environment
/// variables with the same `from_env` + `Default` pattern as
/// `ChatterboxConfig`. `bin_path` and `model_path` are validated to exist
/// because a missing binary or voice should fail loudly at startup, not
/// silently at first `say`. `sample_rate` is not an env var - it's read
/// from the voice's own `<model>.onnx.json` sidecar file, since a wrong
/// value would play audio back at the wrong pitch.
#[derive(Debug, Clone)]
pub struct PiperConfig {
    pub bin_path: PathBuf,
    pub model_path: PathBuf,
    pub sample_rate: u32,
    pub length_scale: f32,
    pub noise_scale: f32,
    pub speaker: Option<String>,
}

/// The parts of a Piper voice's `<model>.onnx.json` sidecar file this
/// adapter cares about. Piper ships one of these next to every `.onnx`
/// model; the rest of the file (phoneme map, espeak config, ...) is
/// irrelevant here so `serde` simply ignores it.
#[derive(Deserialize)]
struct VoiceConfig {
    audio: AudioConfig,
}

#[derive(Deserialize)]
struct AudioConfig {
    sample_rate: u32,
}

impl PiperConfig {
    /// Reads `NALA_PIPER_*` environment variables, falling back to
    /// defaults, and validates that `bin_path` and `model_path` exist and
    /// that the model's `.onnx.json` sidecar parses.
    pub fn from_env() -> Result<Self, SpeechError> {
        let bin_path = PathBuf::from(env_string("NALA_PIPER_BIN", DEFAULT_BIN));
        if !bin_path.exists() {
            return Err(SpeechError::Configuration(format!(
                "Piper binary not found at '{}' (set NALA_PIPER_BIN to override, or run scripts/piper-setup.ps1)",
                bin_path.display()
            )));
        }

        let model_path = PathBuf::from(env_string("NALA_PIPER_MODEL", DEFAULT_MODEL));
        if !model_path.exists() {
            return Err(SpeechError::Configuration(format!(
                "Piper voice model not found at '{}' (set NALA_PIPER_MODEL to override, or run scripts/piper-setup.ps1)",
                model_path.display()
            )));
        }

        let sample_rate = read_sample_rate(&model_path)?;

        Ok(Self {
            bin_path,
            model_path,
            sample_rate,
            length_scale: env_f32("NALA_PIPER_LENGTH_SCALE", DEFAULT_LENGTH_SCALE),
            noise_scale: env_f32("NALA_PIPER_NOISE_SCALE", DEFAULT_NOISE_SCALE),
            speaker: std::env::var("NALA_PIPER_SPEAKER").ok(),
        })
    }
}

/// Reads `audio.sample_rate` from `<model_path>.json`, the voice config
/// sidecar Piper ships beside every `.onnx` file.
fn read_sample_rate(model_path: &std::path::Path) -> Result<u32, SpeechError> {
    let config_path = {
        let mut path = model_path.as_os_str().to_os_string();
        path.push(".json");
        PathBuf::from(path)
    };

    let contents = fs::read_to_string(&config_path).map_err(|error| {
        SpeechError::Configuration(format!(
            "could not read Piper voice config '{}': {error}",
            config_path.display()
        ))
    })?;

    let config: VoiceConfig = serde_json::from_str(&contents).map_err(|error| {
        SpeechError::Configuration(format!(
            "could not parse Piper voice config '{}': {error}",
            config_path.display()
        ))
    })?;

    Ok(config.audio.sample_rate)
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
