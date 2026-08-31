use std::io::Read;
use std::time::Duration;

use crate::ports::http::{HttpError, HttpFetcher};

const TIMEOUT: Duration = Duration::from_secs(15);
const USER_AGENT: &str = "nala-assistant/0.1";
/// Caps the size of a fetched body so a huge page can't blow up memory or a
/// tool's output. 2MB is generous for the JSON/HTML pages these tools fetch.
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;

pub struct ReqwestFetcher {
    client: reqwest::blocking::Client,
}

impl ReqwestFetcher {
    pub fn new() -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(TIMEOUT)
            .user_agent(USER_AGENT)
            .build()
            .expect("failed to build reqwest client");

        Self { client }
    }
}

impl Default for ReqwestFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpFetcher for ReqwestFetcher {
    fn get(&self, url: &str) -> Result<String, HttpError> {
        let response = self
            .client
            .get(url)
            .send()
            .map_err(|error| HttpError::Request(error.to_string()))?;

        if !response.status().is_success() {
            return Err(HttpError::Status(response.status().as_u16()));
        }

        let mut body = String::new();
        response
            .take(MAX_BODY_BYTES as u64 + 1)
            .read_to_string(&mut body)
            .map_err(|error| HttpError::Request(error.to_string()))?;

        if body.len() > MAX_BODY_BYTES {
            return Err(HttpError::TooLarge {
                limit: MAX_BODY_BYTES,
            });
        }

        Ok(body)
    }
}
