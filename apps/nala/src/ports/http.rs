/// A minimal, blocking HTTP GET port — the only HTTP verb the web-facing
/// tools (`get_weather`, `web_search`, `fetch_url`) need. Kept separate from
/// any specific client so tests can substitute a fake instead of hitting the
/// network.
pub trait HttpFetcher {
    fn get(&self, url: &str) -> Result<String, HttpError>;
}

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("request failed: {0}")]
    Request(String),
    #[error("unexpected status {0}")]
    Status(u16),
    #[error("response body too large (limit is {limit} bytes)")]
    TooLarge { limit: usize },
}
