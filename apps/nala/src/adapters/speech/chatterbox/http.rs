use std::time::Duration;

use reqwest::blocking::Client;
use serde::Serialize;

use crate::ports::speech::SpeechError;

use super::SynthesizeSpeech;

/// HTTP client for a Chatterbox TTS server (compatible with the
/// `chatterbox-tts-api` FastAPI wrapper's `/v1/audio/speech` contract). The
/// server holds the voice reference; this client only ever sends the voice
/// name, never the reference audio itself.
pub struct HttpChatterbox {
    client: Client,
    base_url: String,
    voice: String,
    language: String,
    exaggeration: f32,
    cfg_weight: f32,
    temperature: f32,
}

impl HttpChatterbox {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        base_url: &str,
        voice: &str,
        language: &str,
        exaggeration: f32,
        cfg_weight: f32,
        temperature: f32,
        timeout: Duration,
    ) -> Self {
        Self {
            client: Client::builder()
                .timeout(timeout)
                .build()
                .expect("reqwest client with a timeout should always build"),
            base_url: base_url.to_string(),
            voice: voice.to_string(),
            language: language.to_string(),
            exaggeration,
            cfg_weight,
            temperature,
        }
    }
}

#[derive(Serialize)]
struct SpeechRequest<'a> {
    input: &'a str,
    voice: &'a str,
    language: &'a str,
    exaggeration: f32,
    cfg_weight: f32,
    temperature: f32,
}

impl SynthesizeSpeech for HttpChatterbox {
    fn synthesize(&self, text: &str) -> Result<Vec<u8>, SpeechError> {
        let request = SpeechRequest {
            input: text,
            voice: &self.voice,
            language: &self.language,
            exaggeration: self.exaggeration,
            cfg_weight: self.cfg_weight,
            temperature: self.temperature,
        };

        let response = self
            .client
            .post(format!("{}/v1/audio/speech", self.base_url))
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

        let bytes = response
            .bytes()
            .map_err(|error| SpeechError::Synthesis(error.to_string()))?;

        if !status.is_success() {
            return Err(SpeechError::Synthesis(format!(
                "Chatterbox rejected the request with status {status}"
            )));
        }

        if bytes.is_empty() {
            return Err(SpeechError::Synthesis(
                "Chatterbox returned an empty audio body".to_string(),
            ));
        }

        Ok(bytes.to_vec())
    }
}
