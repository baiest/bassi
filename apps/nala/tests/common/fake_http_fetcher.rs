use std::cell::RefCell;

use nala::ports::http::{HttpError, HttpFetcher};

/// Returns queued responses in order (one per call to `get`), and records
/// every URL requested so tests can assert on what was fetched.
#[derive(Default)]
pub struct FakeHttpFetcher {
    responses: RefCell<Vec<Result<String, HttpError>>>,
    pub requested_urls: RefCell<Vec<String>>,
}

impl FakeHttpFetcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues responses in call order: the first `get` returns the first
    /// queued response, and so on.
    pub fn with_responses(responses: Vec<Result<String, HttpError>>) -> Self {
        Self {
            responses: RefCell::new(responses),
            requested_urls: RefCell::new(Vec::new()),
        }
    }
}

impl HttpFetcher for FakeHttpFetcher {
    fn get(&self, url: &str) -> Result<String, HttpError> {
        self.requested_urls.borrow_mut().push(url.to_string());

        if self.responses.borrow().is_empty() {
            return Err(HttpError::Request("no response queued".to_string()));
        }

        self.responses.borrow_mut().remove(0)
    }
}
