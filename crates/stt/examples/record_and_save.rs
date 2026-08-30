fn main() {
    let audio = stt::record_until_enter().expect("recording failed");

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: audio.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create("test_recording.wav", spec).unwrap();
    for sample in audio.samples {
        writer
            .write_sample((sample * i16::MAX as f32) as i16)
            .unwrap();
    }
    writer.finalize().unwrap();

    println!("Saved to test_recording.wav");
}
