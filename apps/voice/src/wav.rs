//! Encodes/decodes the WAV audio that crosses the wire to/from a phone
//! client: mono PCM16 in, mono/stereo PCM16 out (whatever the TTS backend's
//! `PcmStream` reports), always via `hound` rather than a hand-rolled
//! header parser.

#[derive(Debug, thiserror::Error)]
pub enum WavError {
    #[error("invalid WAV data: {0}")]
    Invalid(String),
    #[error("expected mono audio, got {0} channel(s)")]
    NotMono(u16),
    #[error("expected {expected} Hz audio, got {actual} Hz")]
    WrongSampleRate { expected: u32, actual: u32 },
}

/// Decodes `bytes` as a WAV file and returns its samples as `f32` in
/// `[-1.0, 1.0]` — the format `stt::Transcribe` expects. Rejects anything
/// that isn't mono at `expected_sample_rate`, since a silent resample here
/// would let a misconfigured client degrade transcription without any
/// visible error.
pub fn decode_wav(bytes: &[u8], expected_sample_rate: u32) -> Result<Vec<f32>, WavError> {
    let cursor = std::io::Cursor::new(bytes);
    let mut reader =
        hound::WavReader::new(cursor).map_err(|error| WavError::Invalid(error.to_string()))?;
    let spec = reader.spec();

    if spec.channels != 1 {
        return Err(WavError::NotMono(spec.channels));
    }
    if spec.sample_rate != expected_sample_rate {
        return Err(WavError::WrongSampleRate {
            expected: expected_sample_rate,
            actual: spec.sample_rate,
        });
    }

    let samples: Result<Vec<f32>, _> = match spec.sample_format {
        hound::SampleFormat::Int => reader
            .samples::<i16>()
            .map(|sample| sample.map(|sample| sample as f32 / i16::MAX as f32))
            .collect(),
        hound::SampleFormat::Float => reader.samples::<f32>().collect(),
    };

    samples.map_err(|error| WavError::Invalid(error.to_string()))
}

/// Encodes interleaved 16-bit PCM samples as a WAV file — the format a
/// phone client plays directly with no further decoding.
pub fn encode_wav(samples: &[i16], sample_rate: u32, channels: u16) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut writer =
            hound::WavWriter::new(&mut cursor, spec).expect("in-memory WAV writer never fails");
        for &sample in samples {
            writer
                .write_sample(sample)
                .expect("in-memory WAV writer never fails");
        }
        writer.finalize().expect("in-memory WAV writer never fails");
    }
    cursor.into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_samples_through_encode_and_decode() {
        let samples: Vec<i16> = vec![0, 1000, -1000, i16::MAX, i16::MIN + 1];

        let wav = encode_wav(&samples, 16_000, 1);
        let decoded = decode_wav(&wav, 16_000).expect("should decode what we just encoded");

        assert_eq!(decoded.len(), samples.len());
        for (original, decoded) in samples.iter().zip(decoded.iter()) {
            let expected = *original as f32 / i16::MAX as f32;
            assert!((decoded - expected).abs() < 1e-4);
        }
    }

    #[test]
    fn rejects_stereo_audio() {
        let wav = encode_wav(&[0, 0, 0, 0], 16_000, 2);

        let error = decode_wav(&wav, 16_000).unwrap_err();

        assert!(matches!(error, WavError::NotMono(2)));
    }

    #[test]
    fn rejects_the_wrong_sample_rate() {
        let wav = encode_wav(&[0, 0], 48_000, 1);

        let error = decode_wav(&wav, 16_000).unwrap_err();

        assert!(matches!(
            error,
            WavError::WrongSampleRate {
                expected: 16_000,
                actual: 48_000
            }
        ));
    }

    #[test]
    fn rejects_garbage_bytes() {
        let error = decode_wav(b"not a wav file", 16_000).unwrap_err();

        assert!(matches!(error, WavError::Invalid(_)));
    }

    #[test]
    fn encode_of_an_empty_sample_list_still_produces_a_valid_wav_header() {
        let wav = encode_wav(&[], 16_000, 1);

        let decoded = decode_wav(&wav, 16_000).expect("an empty WAV is still valid");

        assert!(decoded.is_empty());
    }
}
