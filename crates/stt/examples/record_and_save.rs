/// Records one push-to-talk clip and saves it as a .wav. Independent of
/// `apps/voice`'s always-listening pipeline — run this from a separate
/// console as many times as needed (e.g. to collect wake-phrase reference
/// samples) without touching a running `voice.exe`.
///
/// Usage: `cargo run -p stt --example record_and_save -- <output.wav>`
/// (defaults to `test_recording.wav` if no path is given).
fn main() {
    let output_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "test_recording.wav".to_string());

    println!("Presioná Enter para empezar a grabar...");
    let audio = stt::record_until_enter().expect("recording failed");

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: audio.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(&output_path, spec).unwrap();
    for sample in audio.samples {
        writer
            .write_sample((sample * i16::MAX as f32) as i16)
            .unwrap();
    }
    writer.finalize().unwrap();

    println!("Saved to {output_path}");
}
