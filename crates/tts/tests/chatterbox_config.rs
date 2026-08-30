use std::sync::Mutex;

use tts::chatterbox::config::ChatterboxConfig;

// Environment variables are process-global, so config tests must not run
// concurrently with each other.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn clear_env() {
    for key in [
        "NALA_CHATTERBOX_URL",
        "NALA_CHATTERBOX_VOICE",
        "NALA_CHATTERBOX_REFERENCE",
        "NALA_CHATTERBOX_LANGUAGE",
        "NALA_CHATTERBOX_EXAGGERATION",
        "NALA_CHATTERBOX_CFG_WEIGHT",
        "NALA_CHATTERBOX_TEMPERATURE",
        "NALA_CHATTERBOX_TIMEOUT_S",
        "NALA_CHATTERBOX_READ_TIMEOUT_S",
        "NALA_CHATTERBOX_STREAMING_STRATEGY",
        "NALA_CHATTERBOX_CHUNK_SIZE",
        "NALA_CHATTERBOX_AUTOSTART",
        "NALA_CHATTERBOX_CMD",
        "NALA_CHATTERBOX_STARTUP_TIMEOUT_S",
    ] {
        unsafe { std::env::remove_var(key) };
    }
}

fn existing_reference_path() -> String {
    // Cargo always runs tests with CWD = the crate root, so this file exists
    // during `cargo test` regardless of caller's working directory.
    "Cargo.toml".to_string()
}

#[test]
fn config_uses_defaults_when_env_absent() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_env();
    unsafe { std::env::set_var("NALA_CHATTERBOX_REFERENCE", existing_reference_path()) };

    let config = ChatterboxConfig::from_env().expect("defaults should be valid");

    assert_eq!(config.base_url, "http://127.0.0.1:4123");
    assert_eq!(config.voice, "nala");
    assert_eq!(config.language, "es");
    assert_eq!(config.exaggeration, 0.5);
    assert_eq!(config.cfg_weight, 0.5);
    assert_eq!(config.temperature, 0.8);
    assert_eq!(config.timeout, std::time::Duration::from_secs(30));
    assert_eq!(config.read_timeout, std::time::Duration::from_secs(60));
    assert_eq!(config.streaming_strategy, "sentence");
    assert_eq!(config.streaming_chunk_size, 200);
    assert!(config.autostart);
    assert_eq!(config.startup_timeout, std::time::Duration::from_secs(180));

    clear_env();
}

#[test]
fn config_reads_overrides_from_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_env();
    unsafe {
        std::env::set_var("NALA_CHATTERBOX_URL", "http://example.local:9000");
        std::env::set_var("NALA_CHATTERBOX_VOICE", "custom-voice");
        std::env::set_var("NALA_CHATTERBOX_REFERENCE", existing_reference_path());
        std::env::set_var("NALA_CHATTERBOX_LANGUAGE", "en");
        std::env::set_var("NALA_CHATTERBOX_EXAGGERATION", "0.9");
        std::env::set_var("NALA_CHATTERBOX_CFG_WEIGHT", "0.2");
        std::env::set_var("NALA_CHATTERBOX_TEMPERATURE", "1.1");
        std::env::set_var("NALA_CHATTERBOX_TIMEOUT_S", "5");
        std::env::set_var("NALA_CHATTERBOX_READ_TIMEOUT_S", "90");
        std::env::set_var("NALA_CHATTERBOX_STREAMING_STRATEGY", "paragraph");
        std::env::set_var("NALA_CHATTERBOX_CHUNK_SIZE", "350");
        std::env::set_var("NALA_CHATTERBOX_AUTOSTART", "0");
        std::env::set_var("NALA_CHATTERBOX_STARTUP_TIMEOUT_S", "10");
    }

    let config = ChatterboxConfig::from_env().expect("overrides should be valid");

    assert_eq!(config.base_url, "http://example.local:9000");
    assert_eq!(config.voice, "custom-voice");
    assert_eq!(config.language, "en");
    assert_eq!(config.exaggeration, 0.9);
    assert_eq!(config.cfg_weight, 0.2);
    assert_eq!(config.temperature, 1.1);
    assert_eq!(config.timeout, std::time::Duration::from_secs(5));
    assert_eq!(config.read_timeout, std::time::Duration::from_secs(90));
    assert_eq!(config.streaming_strategy, "paragraph");
    assert_eq!(config.streaming_chunk_size, 350);
    assert!(!config.autostart);
    assert_eq!(config.startup_timeout, std::time::Duration::from_secs(10));

    clear_env();
}

#[test]
fn config_fails_when_reference_wav_missing() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_env();
    unsafe {
        std::env::set_var(
            "NALA_CHATTERBOX_REFERENCE",
            "definitely/does/not/exist/reference.wav",
        )
    };

    let error = ChatterboxConfig::from_env().expect_err("missing reference.wav should error");

    let message = error.to_string();
    assert!(message.contains("reference.wav") || message.contains("does/not/exist"));

    clear_env();
}
