use std::io::Read;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use reqwest::blocking::Client;
use serde::Serialize;

use crate::ports::speech::SpeechError;

use super::StreamSynthesizeSpeech;
use super::pcm::PcmStream;

/// Number of bytes to read from the response body per `read` call. Small
/// enough to keep playback latency low (the player can start on the first
/// full sample as soon as it arrives), large enough to avoid excessive
/// syscall overhead.
const READ_CHUNK_BYTES: usize = 8192;

/// HTTP client for a Chatterbox TTS server (compatible with the
/// `chatterbox-tts-api` FastAPI wrapper's `/v1/audio/speech/stream`
/// contract). The server holds the voice reference; this client only ever
/// sends the voice name, never the reference audio itself.
pub struct HttpChatterbox {
    client: Client,
    base_url: String,
    voice: String,
    exaggeration: f32,
    cfg_weight: f32,
    temperature: f32,
    streaming_strategy: String,
    streaming_chunk_size: u32,
    read_timeout: Duration,
}

impl HttpChatterbox {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        base_url: &str,
        voice: &str,
        exaggeration: f32,
        cfg_weight: f32,
        temperature: f32,
        streaming_strategy: &str,
        streaming_chunk_size: u32,
        timeout: Duration,
        read_timeout: Duration,
    ) -> Self {
        Self {
            client: Client::builder()
                .timeout(timeout)
                .build()
                .expect("reqwest client with a timeout should always build"),
            base_url: base_url.to_string(),
            voice: voice.to_string(),
            exaggeration,
            cfg_weight,
            temperature,
            streaming_strategy: streaming_strategy.to_string(),
            streaming_chunk_size,
            read_timeout,
        }
    }
}

#[derive(Serialize)]
struct StreamRequest<'a> {
    input: &'a str,
    voice: &'a str,
    exaggeration: f32,
    cfg_weight: f32,
    temperature: f32,
    streaming_strategy: &'a str,
    streaming_chunk_size: u32,
}

/// A parsed canonical PCM WAV header: sample rate, channel count, and
/// where sample data starts. Extracted as a pure function so header parsing
/// can be unit tested against byte fixtures without a server.
struct WavHeader {
    sample_rate: u32,
    channels: u16,
}

fn parse_wav_header(bytes: &[u8]) -> Result<WavHeader, SpeechError> {
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(SpeechError::Synthesis(
            "Chatterbox response is not a WAV stream".to_string(),
        ));
    }
    if &bytes[12..16] != b"fmt " {
        return Err(SpeechError::Synthesis(
            "Chatterbox WAV header is missing the 'fmt ' chunk".to_string(),
        ));
    }

    let audio_format = u16::from_le_bytes([bytes[20], bytes[21]]);
    let channels = u16::from_le_bytes([bytes[22], bytes[23]]);
    let sample_rate = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
    let bits_per_sample = u16::from_le_bytes([bytes[34], bytes[35]]);

    if audio_format != 1 {
        return Err(SpeechError::Synthesis(format!(
            "Chatterbox WAV stream uses unsupported audio format {audio_format} (expected PCM)"
        )));
    }
    if bits_per_sample != 16 {
        return Err(SpeechError::Synthesis(format!(
            "Chatterbox WAV stream uses unsupported bit depth {bits_per_sample} (expected 16)"
        )));
    }

    Ok(WavHeader {
        sample_rate,
        channels,
    })
}

/// Converts a little-endian byte buffer into `i16` samples. `leftover`
/// carries a dangling odd byte from one call to the next, since chunk
/// boundaries from the network never line up with 2-byte sample boundaries.
fn bytes_to_samples(bytes: &[u8], leftover: &mut Option<u8>) -> Vec<i16> {
    let mut samples = Vec::with_capacity(bytes.len() / 2 + 1);
    let mut iter = bytes.iter().copied();

    let mut pending = leftover.take();
    loop {
        let low = match pending.take() {
            Some(byte) => byte,
            None => match iter.next() {
                Some(byte) => byte,
                None => break,
            },
        };
        match iter.next() {
            Some(high) => samples.push(i16::from_le_bytes([low, high])),
            None => {
                *leftover = Some(low);
                break;
            }
        }
    }

    samples
}

impl StreamSynthesizeSpeech for HttpChatterbox {
    fn synthesize_stream(&self, text: &str) -> Result<PcmStream, SpeechError> {
        let request = StreamRequest {
            input: text,
            voice: &self.voice,
            exaggeration: self.exaggeration,
            cfg_weight: self.cfg_weight,
            temperature: self.temperature,
            streaming_strategy: &self.streaming_strategy,
            streaming_chunk_size: self.streaming_chunk_size,
        };

        let mut response = self
            .client
            .post(format!("{}/v1/audio/speech/stream", self.base_url))
            .timeout(self.read_timeout)
            .json(&request)
            .send()
            .map_err(|error| SpeechError::Unavailable(error.to_string()))?;

        let status = response.status();

        if status.is_server_error() {
            let body = response.text().unwrap_or_default();
            return Err(SpeechError::Unavailable(format!(
                "Chatterbox returned status {status}: {body}"
            )));
        }
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(SpeechError::Synthesis(format!(
                "Chatterbox rejected the request with status {status}: {body}"
            )));
        }

        let mut header_buf = [0u8; 44];
        response.read_exact(&mut header_buf).map_err(|error| {
            SpeechError::Synthesis(format!("failed to read Chatterbox WAV header: {error}"))
        })?;
        let header = parse_wav_header(&header_buf)?;

        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || stream_samples(response, sender));

        Ok(PcmStream {
            sample_rate: header.sample_rate,
            channels: header.channels,
            chunks: receiver,
        })
    }
}

fn stream_samples(
    mut response: reqwest::blocking::Response,
    sender: mpsc::Sender<Result<Vec<i16>, SpeechError>>,
) {
    let mut buffer = [0u8; READ_CHUNK_BYTES];
    let mut leftover = None;
    let mut sent_any_samples = false;

    loop {
        match response.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                let samples = bytes_to_samples(&buffer[..n], &mut leftover);
                if !samples.is_empty() {
                    sent_any_samples = true;
                    if sender.send(Ok(samples)).is_err() {
                        // Receiver gone (playback stopped listening) - no
                        // point reading more of the response.
                        return;
                    }
                }
            }
            Err(error) => {
                let _ = sender.send(Err(SpeechError::Synthesis(format!(
                    "Chatterbox stream read failed: {error}"
                ))));
                return;
            }
        }
    }

    if !sent_any_samples {
        let _ = sender.send(Err(SpeechError::Synthesis(
            "Chatterbox returned an empty audio stream".to_string(),
        )));
    }
}
