//! Runs Nala as a server: each accepted connection gets its own `Assistant`
//! (isolated transcript) and speaks `agent_protocol::{ClientMessage,
//! ServerMessage}` over it. Kept fully synchronous — one thread per
//! connection — since nothing else in this workspace uses an async runtime
//! and `Assistant::process` is itself blocking.

use std::io;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use agent_protocol::{ClientMessage, Event, EventSink, ServerMessage};
use tungstenite::{Message, WebSocket};

use crate::application::assistant::Assistant;
use crate::bootstrap;
use crate::ports::llm::Llm;
use crate::ports::tool_dispatcher::{ToolDispatcher, ToolOutcome};

/// One connection's transport: receiving a client message and sending a
/// server message. A trait (rather than using `tungstenite::WebSocket`
/// directly) so `run_session` can be tested with an in-memory fake instead
/// of a real socket.
pub trait Wire {
    /// `Ok(None)` means the client closed the connection cleanly.
    fn recv(&mut self) -> io::Result<Option<ClientMessage>>;
    fn send(&mut self, message: ServerMessage) -> io::Result<()>;
}

impl<S: std::io::Read + std::io::Write> Wire for WebSocket<S> {
    fn recv(&mut self) -> io::Result<Option<ClientMessage>> {
        loop {
            match self.read() {
                Ok(Message::Text(text)) => {
                    let message = serde_json::from_str(&text)
                        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                    return Ok(Some(message));
                }
                // Binary/ping/pong/frame noise: not a client message, keep
                // waiting for the next one instead of erroring out.
                Ok(Message::Close(_)) => return Ok(None),
                Ok(_) => continue,
                Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                    return Ok(None);
                }
                Err(error) => return Err(io::Error::other(error)),
            }
        }
    }

    fn send(&mut self, message: ServerMessage) -> io::Result<()> {
        let json = serde_json::to_string(&message)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        WebSocket::send(self, Message::Text(json)).map_err(io::Error::other)
    }
}

/// Serializes every emitted `Event` and pushes it over `wire` immediately,
/// so a client watching the connection sees progress narration while
/// `Assistant::process` is still running. Shared via `Arc<Mutex<_>>` with the
/// session loop, which also uses the same wire to send the final `Reply`.
pub struct WsEventSink<W> {
    wire: Arc<Mutex<W>>,
}

impl<W> WsEventSink<W> {
    pub fn new(wire: Arc<Mutex<W>>) -> Self {
        Self { wire }
    }
}

impl<W: Wire> EventSink for WsEventSink<W> {
    fn emit(&mut self, event: Event) {
        // A send failure here just means the client won't see this one
        // progress update — the turn itself keeps running, and the final
        // Reply/Error send (in `run_session`) is what surfaces a truly dead
        // connection.
        let _ = self.wire.lock().unwrap().send(ServerMessage::Event(event));
    }
}

/// Runs one connection's session loop: read an `Input`, run it through the
/// assistant (whose events stream out via `wire` as they happen), send back
/// the `Reply`/`Error`, repeat until the client disconnects.
pub fn run_session<L, D, W>(mut assistant: Assistant<L, D, WsEventSink<W>>, wire: Arc<Mutex<W>>)
where
    L: Llm + Send + 'static,
    D: ToolDispatcher<Output = ToolOutcome>,
    D::Error: std::error::Error + 'static,
    W: Wire,
{
    loop {
        let message = wire.lock().unwrap().recv();
        match message {
            Ok(Some(ClientMessage::Input { text })) => {
                let outcome = match assistant.process(&text) {
                    Ok(text) => ServerMessage::Reply { text },
                    Err(error) => ServerMessage::Error {
                        message: error.to_string(),
                    },
                };
                let _ = wire.lock().unwrap().send(outcome);
            }
            // No cancellation support over the wire yet — the local Ctrl+C
            // signal is the only cancel source today; a remote client's
            // Cancel is accepted but ignored rather than rejected, so a
            // future implementation can turn it on without breaking older
            // clients that already send it.
            Ok(Some(ClientMessage::Cancel)) => {}
            Ok(None) => break,
            Err(_) => break,
        }
    }
}

fn handle_connection(stream: TcpStream) {
    let ws = match tungstenite::accept(stream) {
        Ok(ws) => ws,
        Err(error) => {
            eprintln!("Warning: WebSocket handshake failed: {error}");
            return;
        }
    };

    let wire = Arc::new(Mutex::new(ws));
    let events = WsEventSink::new(Arc::clone(&wire));
    let assistant = bootstrap::build_assistant(events);
    let (assistant, _cancel_signal) = bootstrap::install_cancel_signal(assistant);

    run_session(assistant, wire);
}

/// Binds `addr` and serves one `Assistant` session per accepted connection
/// on its own thread, forever.
pub fn serve(addr: &str) -> io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    println!("Nala listening on ws://{addr}");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(move || handle_connection(stream));
            }
            Err(error) => eprintln!("Warning: failed to accept a connection: {error}"),
        }
    }

    Ok(())
}
