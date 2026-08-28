use std::collections::VecDeque;

use nala::adapters::mcp::stdio::Transport;

/// An in-memory `Transport` for testing `StdioMcpClient` without spawning a
/// real process. `responses` is what `read_line` returns, in order;
/// `sent` records every line passed to `send_line`, in order.
#[derive(Default)]
pub struct FakeTransport {
    pub responses: VecDeque<String>,
    pub sent: Vec<String>,
    pub fail_read: bool,
}

impl FakeTransport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_responses(responses: Vec<&str>) -> Self {
        Self {
            responses: responses.into_iter().map(|line| line.to_string()).collect(),
            sent: Vec::new(),
            fail_read: false,
        }
    }
}

impl Transport for FakeTransport {
    fn send_line(&mut self, line: &str) -> std::io::Result<()> {
        self.sent.push(line.to_string());
        Ok(())
    }

    fn read_line(&mut self) -> std::io::Result<String> {
        if self.fail_read {
            return Err(std::io::Error::other("fake transport read failed"));
        }

        self.responses
            .pop_front()
            .ok_or_else(|| std::io::Error::other("no more fake responses"))
    }
}
