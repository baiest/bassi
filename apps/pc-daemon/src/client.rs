use device_protocol::{DeviceMessage, NalaMessage};

#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("could not connect to nala: {0}")]
    Connect(String),
    #[error("connection to nala failed: {0}")]
    Io(String),
}

/// One connection's transport, from the daemon's side: sending a
/// `DeviceMessage` and receiving a `NalaMessage`. A trait (rather than
/// using `tungstenite::WebSocket` directly) so `run_session` can be tested
/// with an in-memory fake instead of a real socket — mirrors `nala`'s own
/// `Wire` trait in `server.rs`.
pub trait DeviceWire {
    fn send(&mut self, message: &DeviceMessage) -> Result<(), DaemonError>;
    /// `Ok(None)` means the connection closed.
    fn recv(&mut self) -> Result<Option<NalaMessage>, DaemonError>;
}
