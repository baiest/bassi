use std::sync::Mutex;

use tts::SpeechError;
use tts::piper::config::PiperConfig;

// Environment variables are process-global, so config tests must not run
// concurrently with each other.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn clear_env() {
    for key in [
        "NALA_PIPER_BIN",
        "NALA_PIPER_MODEL",
        "NALA_PIPER_LENGTH_SCALE",
        "NALA_PIPER_NOISE_SCALE",
        "NALA_PIPER_SPEAKER",
    ] {
        unsafe { std::env::remove_var(key) };
    }
}

// Cargo always runs tests with CWD = the crate root, so these fixture
// paths exist during `cargo test` regardless of the caller's working
// directory. `Cargo.toml` stands in for the Piper binary - only its
// existence is checked, never its content.
fn existing_bin_path() -> String {
    "Cargo.toml".to_string()
}

fn existing_model_path() -> String {
    "tests/fixtures/piper/model.onnx".to_string()
}

#[test]
fn config_uses_defaults_when_env_absent() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_env();
    unsafe {
        std::env::set_var("NALA_PIPER_BIN", existing_bin_path());
        std::env::set_var("NALA_PIPER_MODEL", existing_model_path());
    }

    let config = PiperConfig::from_env().expect("defaults should be valid");

    assert_eq!(config.sample_rate, 22_050);
    assert_eq!(config.length_scale, 1.0);
    assert_eq!(config.noise_scale, 0.667);
    assert_eq!(config.speaker, None);

    clear_env();
}

#[test]
fn config_reads_overrides_from_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_env();
    unsafe {
        std::env::set_var("NALA_PIPER_BIN", existing_bin_path());
        std::env::set_var("NALA_PIPER_MODEL", existing_model_path());
        std::env::set_var("NALA_PIPER_LENGTH_SCALE", "0.8");
        std::env::set_var("NALA_PIPER_NOISE_SCALE", "0.5");
        std::env::set_var("NALA_PIPER_SPEAKER", "3");
    }

    let config = PiperConfig::from_env().expect("overrides should be valid");

    assert_eq!(config.length_scale, 0.8);
    assert_eq!(config.noise_scale, 0.5);
    assert_eq!(config.speaker, Some("3".to_string()));

    clear_env();
}

#[test]
fn config_fails_when_binary_missing() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_env();
    unsafe {
        std::env::set_var("NALA_PIPER_BIN", "definitely/does/not/exist/piper.exe");
        std::env::set_var("NALA_PIPER_MODEL", existing_model_path());
    }

    let error = PiperConfig::from_env().expect_err("missing binary should error");

    assert!(error.to_string().contains("does/not/exist"));

    clear_env();
}

#[test]
fn config_fails_when_model_missing() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_env();
    unsafe {
        std::env::set_var("NALA_PIPER_BIN", existing_bin_path());
        std::env::set_var("NALA_PIPER_MODEL", "definitely/does/not/exist/voice.onnx");
    }

    let error = PiperConfig::from_env().expect_err("missing model should error");

    assert!(error.to_string().contains("does/not/exist"));

    clear_env();
}

#[test]
fn config_fails_when_voice_config_is_malformed() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_env();
    unsafe {
        std::env::set_var("NALA_PIPER_BIN", existing_bin_path());
        std::env::set_var("NALA_PIPER_MODEL", "tests/fixtures/piper/malformed.onnx");
    }

    let error = PiperConfig::from_env().expect_err("malformed voice config should error");

    assert!(matches!(error, SpeechError::Configuration(_)));

    clear_env();
}
