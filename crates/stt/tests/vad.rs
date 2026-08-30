//! Exercises the real Silero VAD against the committed recording, to
//! prove the ONNX Runtime integration works and that speech scores
//! clearly higher than silence. Ignored by default like the Whisper test:
//! it needs ONNX Runtime, which CI doesn't provide.

use stt::{CHUNK_SAMPLES, SileroVad, VoiceDetector};

fn fixture_samples() -> Vec<f32> {
    let mut reader =
        hound::WavReader::open("tests/fixtures/test_recording.wav").expect("fixture should exist");

    reader
        .samples::<i16>()
        .map(|sample| sample.unwrap() as f32 / i16::MAX as f32)
        .collect()
}

#[test]
#[ignore] // needs ONNX Runtime — run manually with --ignored
fn scores_recorded_speech_higher_than_silence() {
    let mut vad = SileroVad::new().expect("VAD should build");

    let speech: Vec<f32> = fixture_samples();
    let mut speech_hits = 0;
    let mut chunks = 0;
    for chunk in speech.as_chunks::<CHUNK_SAMPLES>().0 {
        if vad.probability(chunk) > 0.5 {
            speech_hits += 1;
        }
        chunks += 1;
    }

    let mut silence_vad = SileroVad::new().expect("VAD should build");
    let silence = vec![0.0_f32; speech.len()];
    let mut silence_hits = 0;
    for chunk in silence.as_chunks::<CHUNK_SAMPLES>().0 {
        if silence_vad.probability(chunk) > 0.5 {
            silence_hits += 1;
        }
    }

    println!("speech: {speech_hits}/{chunks} chunks, silence: {silence_hits}/{chunks}");

    assert!(
        speech_hits > chunks / 10,
        "expected the recording to register as speech, got {speech_hits}/{chunks}"
    );
    assert_eq!(
        silence_hits, 0,
        "digital silence should never score as speech"
    );
}
