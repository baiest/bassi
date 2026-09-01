//! Runs Nala as a server: each accepted connection gets its own `Assistant`
//! (isolated transcript) and speaks `agent_protocol::{ClientMessage,
//! ServerMessage}` over it. Kept fully synchronous — one thread per
//! connection — since nothing else in this workspace uses an async runtime
//! and `Assistant::process` is itself blocking.

use std::io;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use agent_protocol::{ClientMessage, Event, EventSink, ServerMessage};
use device_protocol::DeviceState;
use tungstenite::{Message, WebSocket};

use crate::adapters::devices::state_broadcast::DeviceStateBroadcaster;
use crate::adapters::events::console::ConsoleEventSink;
use crate::application::assistant::Assistant;
use crate::application::devices::registry::DeviceRegistry;
use crate::bootstrap;
use crate::device_server::Device;
use crate::ports::device::RemoteDevice;
use crate::ports::llm::Llm;
use crate::ports::tool_dispatcher::{ToolDispatcher, ToolOutcome};

/// Nala's own greeting — sent to every newly-connected turn client (see
/// `run_session`) rather than left for each client to hardcode its own
/// opening line.
const GREETING_TEXT: &str = "Hola, en que te puedo ayudar?";

/// How many characters per second the greeting is assumed to take to speak.
/// Devices never see this text or hear this audio (voice is the one
/// actually speaking it) — this is only used to keep a connected device's
/// overlay showing `Speaking` for roughly as long as voice would be, via
/// `notify_devices_of_greeting`.
const GREETING_CHARS_PER_SECOND: f64 = 15.0;

fn estimate_speaking_duration(text: &str) -> Duration {
    Duration::from_secs_f64(text.chars().count() as f64 / GREETING_CHARS_PER_SECOND)
}

/// Mirrors the greeting sent to a newly-connected turn client onto every
/// currently-connected device's overlay: `Speaking` while voice would be
/// saying it, back to `Idle` after — reusing the same fire-and-forget
/// `push_state` a turn's own `TurnState` changes already go through (see
/// `DeviceStateBroadcaster`), so the daemon needs no code of its own for
/// this.
fn notify_devices_of_greeting<D: RemoteDevice + Clone>(devices: &DeviceRegistry<D>, text: &str) {
    let snapshot = devices.snapshot();
    if snapshot.is_empty() {
        return;
    }
    for device in &snapshot {
        device.push_state(DeviceState::Speaking);
    }
    thread::sleep(estimate_speaking_duration(text));
    for device in &snapshot {
        device.push_state(DeviceState::Idle);
    }
}

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
/// Also forwards every event to `inner` — same wrap-and-forward pattern as
/// `CsvMetricsSink` — so the server's own console shows a turn's progress
/// too, not just whatever client happens to be connected.
pub struct WsEventSink<E, W> {
    inner: E,
    wire: Arc<Mutex<W>>,
}

impl<E, W> WsEventSink<E, W> {
    pub fn new(inner: E, wire: Arc<Mutex<W>>) -> Self {
        Self { inner, wire }
    }
}

impl<E: EventSink, W: Wire> EventSink for WsEventSink<E, W> {
    fn emit(&mut self, event: Event) {
        // A send failure here just means the client won't see this one
        // progress update — the turn itself keeps running, and the final
        // Reply/Error send (in `run_session`) is what surfaces a truly dead
        // connection. Still logged (not swallowed) so a broken connection
        // shows up in the server's own console, not just the client's.
        if let Err(error) = self
            .wire
            .lock()
            .unwrap()
            .send(ServerMessage::Event(event.clone()))
        {
            eprintln!("Warning: could not send a progress event to the client: {error}");
        }
        self.inner.emit(event);
    }
}

/// Runs one connection's session loop: read an `Input`, run it through the
/// assistant (whose events stream out via `wire` as they happen), send back
/// the `Reply`/`Error`, repeat until the client disconnects.
pub fn run_session<L, D, ES, W>(mut assistant: Assistant<L, D, ES>, wire: Arc<Mutex<W>>)
where
    L: Llm + Send + 'static,
    D: ToolDispatcher<Output = ToolOutcome>,
    D::Error: std::error::Error + 'static,
    ES: EventSink,
    W: Wire,
{
    if let Err(error) = wire.lock().unwrap().send(ServerMessage::Greeting {
        text: GREETING_TEXT.to_string(),
    }) {
        eprintln!("Warning: could not send the greeting to the client: {error}");
    }

    loop {
        let message = wire.lock().unwrap().recv();
        match message {
            Ok(Some(ClientMessage::Input { text })) => {
                let outcome = match assistant.process(&text) {
                    Ok(text) => ServerMessage::Reply { text },
                    Err(error) => {
                        // Printed here (not just sent to the client) so a
                        // failing turn is visible in the server's own
                        // console — matching what the local REPL does.
                        eprintln!("Error: {error}");
                        ServerMessage::Error {
                            message: error.to_string(),
                        }
                    }
                };
                if let Err(error) = wire.lock().unwrap().send(outcome) {
                    eprintln!("Warning: could not send the reply to the client: {error}");
                }
            }
            // No cancellation support over the wire yet — the local Ctrl+C
            // signal is the only cancel source today; a remote client's
            // Cancel is accepted but ignored rather than rejected, so a
            // future implementation can turn it on without breaking older
            // clients that already send it.
            Ok(Some(ClientMessage::Cancel)) => {}
            Ok(None) => break,
            Err(error) => {
                eprintln!("Warning: connection error, ending this session: {error}");
                break;
            }
        }
    }
}

fn handle_connection(stream: TcpStream, devices: Arc<DeviceRegistry<Device>>) {
    let ws = match tungstenite::accept(stream) {
        Ok(ws) => ws,
        Err(error) => {
            eprintln!("Warning: WebSocket handshake failed: {error}");
            return;
        }
    };

    let wire = Arc::new(Mutex::new(ws));
    let events = WsEventSink::new(ConsoleEventSink, Arc::clone(&wire));
    let events = DeviceStateBroadcaster::new(events, Arc::clone(&devices));
    let assistant = bootstrap::build_assistant(events, &devices);

    // Runs on its own thread so a device slow to react to the greeting
    // never delays the turn client's own session from starting.
    let greeting_devices = Arc::clone(&devices);
    thread::spawn(move || notify_devices_of_greeting(greeting_devices.as_ref(), GREETING_TEXT));

    // Deliberately not `bootstrap::install_cancel_signal` here: that
    // installs a Windows console handler which swallows Ctrl+C (so the
    // default "terminate the process" action never runs) in exchange for
    // being able to cancel the turn currently in flight. A server has no
    // per-turn Ctrl+C to catch — its console Ctrl+C should just stop the
    // process, same as any other server — so installing it here would
    // leave `nala --serve` impossible to stop from its own console.
    run_session(assistant, wire);
}

/// Binds `addr` and serves one `Assistant` session per accepted connection
/// on its own thread, forever. `devices` is shared with the device server
/// (`device_server::serve`, on a different port) so every new turn-client
/// connection sees whatever devices are currently connected.
pub fn serve(addr: &str, devices: Arc<DeviceRegistry<Device>>) -> io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    println!("Nala listening on ws://{addr}");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let devices = Arc::clone(&devices);
                thread::spawn(move || handle_connection(stream, devices));
            }
            Err(error) => eprintln!("Warning: failed to accept a connection: {error}"),
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use device_protocol::{CapabilityDefinition, Outcome};

    #[derive(Clone)]
    struct RecordingDevice {
        pushed: Arc<Mutex<Vec<DeviceState>>>,
    }

    impl RecordingDevice {
        fn new() -> Self {
            Self {
                pushed: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn pushed(&self) -> Vec<DeviceState> {
            self.pushed.lock().unwrap().clone()
        }
    }

    impl RemoteDevice for RecordingDevice {
        fn name(&self) -> &str {
            "fake"
        }

        fn capabilities(&self) -> &[CapabilityDefinition] {
            &[]
        }

        fn invoke(&mut self, _capability: &str, _arguments: &str) -> Outcome {
            Outcome::Ok {
                text: String::new(),
                mutated: false,
            }
        }

        fn push_state(&self, state: DeviceState) {
            self.pushed.lock().unwrap().push(state);
        }
    }

    #[test]
    fn estimate_speaking_duration_is_zero_for_empty_text() {
        assert_eq!(estimate_speaking_duration(""), Duration::ZERO);
    }

    #[test]
    fn estimate_speaking_duration_scales_with_length() {
        let short = estimate_speaking_duration("hi");
        let long = estimate_speaking_duration("hi there, this is quite a bit longer");

        assert!(long > short);
    }

    #[test]
    fn notify_devices_of_greeting_pushes_speaking_then_idle_to_every_device() {
        let registry: DeviceRegistry<RecordingDevice> = DeviceRegistry::new();
        let device = RecordingDevice::new();
        registry.register("pc".to_string(), device.clone());

        notify_devices_of_greeting(&registry, "");

        assert_eq!(
            device.pushed(),
            vec![DeviceState::Speaking, DeviceState::Idle]
        );
    }

    #[test]
    fn notify_devices_of_greeting_with_no_devices_does_nothing() {
        let registry: DeviceRegistry<RecordingDevice> = DeviceRegistry::new();

        notify_devices_of_greeting(&registry, "");
    }
}
