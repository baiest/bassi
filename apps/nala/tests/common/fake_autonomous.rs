use std::sync::{Arc, Mutex};

use nala::ports::autonomous::AutonomousAgent;

/// Records every prompt it was asked to respond to and replies with a
/// fixed string, so tests can assert whether the event loop reached the
/// agent at all -- and with what -- without a real `Assistant`.
#[derive(Clone)]
pub struct RecordingAgent {
    pub prompts: Arc<Mutex<Vec<String>>>,
    reply: String,
}

impl RecordingAgent {
    pub fn new(reply: impl Into<String>) -> Self {
        Self {
            prompts: Arc::new(Mutex::new(Vec::new())),
            reply: reply.into(),
        }
    }

    pub fn call_count(&self) -> usize {
        self.prompts.lock().unwrap().len()
    }
}

impl AutonomousAgent for RecordingAgent {
    fn respond_to(&mut self, prompt: &str) -> Result<String, String> {
        self.prompts.lock().unwrap().push(prompt.to_string());
        Ok(self.reply.clone())
    }
}

/// Always fails, for exercising the event loop's failure path.
pub struct FailingAgent {
    pub error: String,
}

impl AutonomousAgent for FailingAgent {
    fn respond_to(&mut self, _prompt: &str) -> Result<String, String> {
        Err(self.error.clone())
    }
}
