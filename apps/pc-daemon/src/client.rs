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

/// A `DeviceWire` over a real WebSocket connection, mirroring
/// `voice::client::TcpWire`. `run_session` is purely reactive (it never
/// needs to act on its own between messages, unlike `voice`'s audio
/// server, which also has an outbox to drain) so a plain blocking
/// `socket.read()` is enough — no read timeout needed here.
pub struct TcpDeviceWire {
    socket: tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
}

impl TcpDeviceWire {
    pub fn connect(addr: &str) -> Result<Self, DaemonError> {
        let url = format!("ws://{addr}");
        let (socket, _response) =
            tungstenite::connect(&url).map_err(|error| DaemonError::Connect(error.to_string()))?;
        Ok(Self { socket })
    }
}

impl DeviceWire for TcpDeviceWire {
    fn send(&mut self, message: &DeviceMessage) -> Result<(), DaemonError> {
        let json =
            serde_json::to_string(message).map_err(|error| DaemonError::Io(error.to_string()))?;
        self.socket
            .send(tungstenite::Message::Text(json))
            .map_err(|error| DaemonError::Io(error.to_string()))
    }

    fn recv(&mut self) -> Result<Option<NalaMessage>, DaemonError> {
        loop {
            match self.socket.read() {
                Ok(tungstenite::Message::Text(text)) => {
                    let message = serde_json::from_str(&text)
                        .map_err(|error| DaemonError::Io(error.to_string()))?;
                    return Ok(Some(message));
                }
                Ok(tungstenite::Message::Close(_)) => return Ok(None),
                Ok(_) => continue,
                Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                    return Ok(None);
                }
                Err(error) => return Err(DaemonError::Io(error.to_string())),
            }
        }
    }
}
