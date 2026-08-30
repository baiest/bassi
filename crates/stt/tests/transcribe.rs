use stt::Transcriber;

#[test]
#[ignore] // requires the model to be downloaded — run manually with --ignored
fn transcribes_a_known_recording() {
    // `cargo test -p stt` runs with cwd = crates/stt, but the model lives at
    // the repo root's data/whisper/ (that's where scripts/stt-setup.ps1
    // downloads it), two levels up.
    let transcriber =
        Transcriber::load("../../data/whisper/ggml-small.bin").expect("model should load");

    let mut reader =
        hound::WavReader::open("tests/fixtures/test_recording.wav").expect("fixture should exist");
    let samples: Vec<f32> = reader
        .samples::<i16>()
        .map(|s| s.unwrap() as f32 / i16::MAX as f32)
        .collect();

    let text = transcriber.transcribe(&samples).unwrap();
    println!("{text}");
    // test_recording.wav says "...grabando la primera prueba de audio para
    // Nala", not "hola" — match what's actually in the fixture.
    assert!(text.to_lowercase().contains("nala"));
}
