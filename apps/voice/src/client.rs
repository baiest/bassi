//! Talks to Nala over a WebSocket instead of calling it as a library.
//! Nala never sees raw audio — this client only ever sends text and
//! receives text/events back.

use agent_protocol::{ClientMessage, Event, ServerMessage};

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("could not connect to nala: {0}")]
    Connect(String),
    #[error("connection to nala failed: {0}")]
    Io(String),
    #[error("nala reported an error: {0}")]
    Server(String),
    #[error("nala closed the connection without replying")]
    ClosedWithoutReply,
}

/// One turn's worth of communication with a Nala server: send `Input`,
/// forward every `Event` that comes back to `on_event` as it arrives (this
/// is what lets narration stay live during the turn), then return the final
/// reply text.
pub trait Wire {
    fn send(&mut self, message: &ClientMessage) -> Result<(), ClientError>;
    /// `Ok(None)` means the connection closed.
    fn recv(&mut self) -> Result<Option<ServerMessage>, ClientError>;
}

pub struct NalaClient<W> {
    wire: W,
}

impl<W: Wire> NalaClient<W> {
    pub fn new(wire: W) -> Self {
        Self { wire }
    }

    /// Reads the one-time `Event::Greeting` Nala sends right after a
    /// connection opens, before any turn. Must be called before `send()` —
    /// a turn's own recv loop doesn't expect this message, so it would
    /// otherwise be mistaken for noise on whichever turn happens to run
    /// first.
    pub fn recv_greeting(&mut self) -> Result<String, ClientError> {
        match self.wire.recv()? {
            Some(ServerMessage::Event(Event::Greeting { text })) => Ok(text),
            Some(_) => Err(ClientError::Server(
                "expected a Greeting right after connecting".to_string(),
            )),
            None => Err(ClientError::ClosedWithoutReply),
        }
    }

    /// Sends `text` as one turn's input and drives `on_event` for every
    /// progress event Nala emits while processing it, returning the final
    /// reply's text.
    pub fn send(
        &mut self,
        text: &str,
        mut on_event: impl FnMut(Event),
    ) -> Result<String, ClientError> {
        self.wire.send(&ClientMessage::Input {
            text: text.to_string(),
        })?;

        loop {
            match self.wire.recv()? {
                Some(ServerMessage::Event(event)) => on_event(event),
                Some(ServerMessage::Reply { text }) => return Ok(text),
                Some(ServerMessage::Error { message }) => return Err(ClientError::Server(message)),
                None => return Err(ClientError::ClosedWithoutReply),
            }
        }
    }
}

/// A `Wire` over a real WebSocket connection. `MaybeTlsStream` is what
/// `tungstenite::connect` hands back even for a plain `ws://` URL — Nala has
/// no TLS support, so this is always the `Plain` variant in practice.
pub struct TcpWire {
    socket: tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
}

impl TcpWire {
    pub fn connect(addr: &str) -> Result<Self, ClientError> {
        let url = format!("ws://{addr}");
        let (socket, _response) =
            tungstenite::connect(&url).map_err(|error| ClientError::Connect(error.to_string()))?;
        Ok(Self { socket })
    }
}

impl Wire for TcpWire {
    fn send(&mut self, message: &ClientMessage) -> Result<(), ClientError> {
        let json =
            serde_json::to_string(message).map_err(|error| ClientError::Io(error.to_string()))?;
        self.socket
            .send(tungstenite::Message::Text(json))
            .map_err(|error| ClientError::Io(error.to_string()))
    }

    fn recv(&mut self) -> Result<Option<ServerMessage>, ClientError> {
        loop {
            match self.socket.read() {
                Ok(tungstenite::Message::Text(text)) => {
                    let message = serde_json::from_str(&text)
                        .map_err(|error| ClientError::Io(error.to_string()))?;
                    return Ok(Some(message));
                }
                Ok(tungstenite::Message::Close(_)) => return Ok(None),
                Ok(_) => continue,
                Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                    return Ok(None);
                }
                Err(error) => return Err(ClientError::Io(error.to_string())),
            }
        }
    }
}
