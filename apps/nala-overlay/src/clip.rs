//! Decodes a WAV clip into raw samples for [`crate::amplitude`] to walk —
//! kept separate from actually playing it (`playback.rs`, which needs a
//! real audio device and isn't unit-tested here, same as
//! `adapters/process/windows.rs` elsewhere in this workspace) so the
//! decode step itself stays covered.

#[derive(Debug, Clone, PartialEq)]
pub struct DecodedClip {
    pub samples: Vec<i16>,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("invalid WAV data: {0}")]
    Invalid(String),
}

/// Decodes `bytes` as a WAV file, keeping whatever sample rate/channel
/// count its header reports — unlike `voice::wav::decode_wav`, this never
/// rejects stereo or an unexpected rate, since a reply clip's format
/// depends on whatever TTS backend `voice --serve` is running and isn't
/// known up front.
pub fn decode_clip(bytes: &[u8]) -> Result<DecodedClip, DecodeError> {
    let cursor = std::io::Cursor::new(bytes);
    let mut reader =
        hound::WavReader::new(cursor).map_err(|error| DecodeError::Invalid(error.to_string()))?;
    let spec = reader.spec();

    let samples: Result<Vec<i16>, _> = match spec.sample_format {
        hound::SampleFormat::Int => reader.samples::<i16>().collect(),
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|sample| sample.map(|sample| (sample * i16::MAX as f32) as i16))
            .collect(),
    };

    Ok(DecodedClip {
        samples: samples.map_err(|error| DecodeError::Invalid(error.to_string()))?,
        sample_rate: spec.sample_rate,
        channels: spec.channels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_test_wav(samples: &[i16], sample_rate: u32, channels: u16) -> Vec<u8> {
        let spec = hound::WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = hound::WavWriter::new(&mut cursor, spec).unwrap();
            for &sample in samples {
                writer.write_sample(sample).unwrap();
            }
            writer.finalize().unwrap();
        }
        cursor.into_inner()
    }

    #[test]
    fn decodes_mono_samples_and_format() {
        let samples = vec![0, 1000, -1000, i16::MAX];
        let wav = encode_test_wav(&samples, 22_050, 1);

        let decoded = decode_clip(&wav).unwrap();

        assert_eq!(decoded.samples, samples);
        assert_eq!(decoded.sample_rate, 22_050);
        assert_eq!(decoded.channels, 1);
    }

    #[test]
    fn decodes_stereo_without_rejecting_it() {
        let wav = encode_test_wav(&[0, 0, 100, 100], 44_100, 2);

        let decoded = decode_clip(&wav).unwrap();

        assert_eq!(decoded.channels, 2);
        assert_eq!(decoded.samples.len(), 4);
    }

    #[test]
    fn rejects_garbage_bytes() {
        let error = decode_clip(b"not a wav file").unwrap_err();

        assert!(matches!(error, DecodeError::Invalid(_)));
    }
}
