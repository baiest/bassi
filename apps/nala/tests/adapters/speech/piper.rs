use nala::adapters::speech::piper::config::PiperConfig;
use nala::adapters::speech::piper::speech::{build_args, normalize_text};

// The real spawn-and-read path (`PiperSpeech::synthesize_stream`) needs a
// real Piper install to exercise meaningfully - like
// `ChatterboxSupervisor::ensure_running`, it's covered by manual
// verification rather than `cargo test` (see the exclusion note in
// scripts/check_coverage.sh). What's unit-tested here is the pure logic
// around it: the exact argv Piper receives, and how multi-line answers are
// normalized into one utterance before being sent to it.

fn config(speaker: Option<&str>) -> PiperConfig {
    PiperConfig {
        bin_path: "Cargo.toml".into(),
        model_path: "tests/fixtures/piper/model.onnx".into(),
        sample_rate: 22_050,
        length_scale: 1.0,
        noise_scale: 0.667,
        speaker: speaker.map(str::to_string),
    }
}

#[test]
fn build_args_includes_model_and_streaming_flag() {
    let args = build_args(&config(None));

    assert!(args.contains(&"--output_raw".to_string()));
    let model_index = args.iter().position(|a| a == "--model").unwrap();
    assert_eq!(args[model_index + 1], "tests/fixtures/piper/model.onnx");
}

#[test]
fn build_args_includes_tuning_flags() {
    let args = build_args(&config(None));

    let length_index = args.iter().position(|a| a == "--length_scale").unwrap();
    assert_eq!(args[length_index + 1], "1");

    let noise_index = args.iter().position(|a| a == "--noise_scale").unwrap();
    assert_eq!(args[noise_index + 1], "0.667");
}

#[test]
fn build_args_omits_speaker_when_unset() {
    let args = build_args(&config(None));

    assert!(!args.contains(&"--speaker".to_string()));
}

#[test]
fn build_args_includes_speaker_when_set() {
    let args = build_args(&config(Some("3")));

    let speaker_index = args.iter().position(|a| a == "--speaker").unwrap();
    assert_eq!(args[speaker_index + 1], "3");
}

#[test]
fn normalize_text_collapses_newlines_and_whitespace() {
    let text = "Hola,\ncomo estas?\n\nTodo   bien.";

    assert_eq!(normalize_text(text), "Hola, como estas? Todo bien.");
}

#[test]
fn normalize_text_trims_surrounding_whitespace() {
    assert_eq!(normalize_text("  hola  "), "hola");
}
