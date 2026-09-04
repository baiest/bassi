//! Nala's second listener: devices (e.g. the PC daemon) connect here,
//! separate from the turn-client listener in `server.rs`, and speak
//! `device_protocol` instead of `agent_protocol`. Kept fully synchronous —
//! one thread per connection, like `server.rs` — for the same reason:
//! nothing else in this workspace uses an async runtime.

use std::io;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use device_protocol::{DeviceMessage, NalaMessage, PROTOCOL_VERSION, RejectReason};
use tungstenite::{Message, WebSocket};

use crate::adapters::devices::websocket::{DeviceSink, WsDevice};
use crate::application::autonomous::event::AutonomousEvent;
use crate::application::devices::registry::DeviceRegistry;
use crate::ports::autonomous::AutonomousEventQueue;

/// How often a connection's reading thread polls for the next message. A
/// blocking read would hold the wire's lock indefinitely, starving
/// `WsDevice::invoke` (running on a completely different thread — a
/// turn-client's `Assistant::process`) of the chance to send an `Invoke`
/// over the same socket. Same pattern as `voice::audio_server`'s
/// `POLL_INTERVAL`.
const READ_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// How often Nala pings a connected device, reported to it in `Welcome` so
/// it knows what silence to expect before treating the connection as dead
/// on its own side too.
const HEARTBEAT_INTERVAL_MS: u64 = 20_000;

/// What one poll of the connection produced. Mirrors
/// `voice::audio_server::WireEvent`: a timed-out read (`Idle`) is not an
/// error, just "nothing yet, keep polling."
pub enum DeviceEvent {
    Message(DeviceMessage),
    Idle,
    Closed,
}

/// One connection's transport: receiving a `DeviceMessage` and sending a
/// `NalaMessage`. A trait (rather than using `tungstenite::WebSocket`
/// directly) so the session logic can be tested with an in-memory fake.
pub trait DeviceConnection {
    fn poll(&mut self) -> io::Result<DeviceEvent>;
    fn send_message(&mut self, message: &NalaMessage) -> io::Result<()>;
}

impl DeviceConnection for WebSocket<TcpStream> {
    fn poll(&mut self) -> io::Result<DeviceEvent> {
        match self.read() {
            Ok(Message::Text(text)) => {
                let message = serde_json::from_str(&text)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                Ok(DeviceEvent::Message(message))
            }
            Ok(Message::Close(_)) => Ok(DeviceEvent::Closed),
            Ok(_) => Ok(DeviceEvent::Idle),
            Err(tungstenite::Error::Io(error))
                if error.kind() == io::ErrorKind::WouldBlock
                    || error.kind() == io::ErrorKind::TimedOut =>
            {
                Ok(DeviceEvent::Idle)
            }
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                Ok(DeviceEvent::Closed)
            }
            Err(error) => Err(io::Error::other(error)),
        }
    }

    fn send_message(&mut self, message: &NalaMessage) -> io::Result<()> {
        let json = serde_json::to_string(message)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        WebSocket::send(self, Message::Text(json)).map_err(io::Error::other)
    }
}

/// A `DeviceSink` over any shared, lockable `DeviceConnection` — the bridge
/// that lets `WsDevice::invoke`, running on a turn-client's thread, send an
/// `Invoke` down the same socket the device server's reading thread polls.
impl<C: DeviceConnection + Send> DeviceSink for Arc<Mutex<C>> {
    fn send(&self, message: &NalaMessage) -> Result<(), String> {
        self.lock()
            .unwrap()
            .send_message(message)
            .map_err(|error| error.to_string())
    }
}

pub type Device = WsDevice<Arc<Mutex<WebSocket<TcpStream>>>>;

fn handle_connection(
    stream: TcpStream,
    registry: Arc<DeviceRegistry<Device>>,
    token: Option<String>,
    events: Option<Arc<dyn AutonomousEventQueue>>,
) {
    // Short read timeout so `poll()` returns `Idle` instead of blocking
    // forever — `WsDevice::invoke`, called from a completely different
    // thread, needs to acquire this same connection's lock to send an
    // `Invoke` while this thread would otherwise be parked in a blocking
    // read. Same pattern as `voice::audio_server`'s `set_read_timeout`.
    if let Err(error) = stream.set_read_timeout(Some(READ_POLL_INTERVAL)) {
        eprintln!("Warning: could not set a read timeout on the device connection: {error}");
    }

    let ws = match tungstenite::accept(stream) {
        Ok(ws) => ws,
        Err(error) => {
            eprintln!("Warning: device WebSocket handshake failed: {error}");
            return;
        }
    };
    let wire = Arc::new(Mutex::new(ws));

    let hello = match wait_for_hello(&wire) {
        Some(hello) => hello,
        None => return,
    };
    let DeviceMessage::Hello {
        protocol_version,
        device_id,
        name,
        token: hello_token,
        capabilities,
        ..
    } = hello
    else {
        unreachable!("wait_for_hello only returns Hello");
    };

    if let Err(reason) = validate_hello(protocol_version, &hello_token, token.as_deref()) {
        let _ = wire
            .lock()
            .unwrap()
            .send_message(&NalaMessage::Reject { reason });
        eprintln!("Warning: rejected device '{device_id}' ({reason:?})");
        return;
    }

    if wire
        .lock()
        .unwrap()
        .send_message(&NalaMessage::Welcome {
            session_id: device_id.clone(),
            heartbeat_interval_ms: HEARTBEAT_INTERVAL_MS,
        })
        .is_err()
    {
        return;
    }

    let device = WsDevice::new(name, capabilities, Arc::clone(&wire));
    registry.register(device_id.clone(), device.clone());
    println!("Device '{device_id}' connected.");
    publish_device_event(
        events.as_deref(),
        &device_id,
        "device_connected",
        serde_json::Value::Null,
    );

    loop {
        let event = { wire.lock().unwrap().poll() };
        match event {
            Ok(DeviceEvent::Message(DeviceMessage::Result {
                request_id,
                outcome,
            })) => device.deliver_result(&request_id, outcome),
            Ok(DeviceEvent::Message(DeviceMessage::Pong { .. })) => {}
            // A second Hello from an already-connected device is not part
            // of this session's protocol — ignored rather than tearing the
            // connection down over it.
            Ok(DeviceEvent::Message(DeviceMessage::Hello { .. })) => {}
            Ok(DeviceEvent::Message(DeviceMessage::Event { kind, payload })) => {
                publish_device_event(events.as_deref(), &device_id, kind, payload);
            }
            Ok(DeviceEvent::Idle) => {}
            Ok(DeviceEvent::Closed) => break,
            Err(error) => {
                eprintln!("Warning: device connection error, ending this session: {error}");
                break;
            }
        }
    }

    registry.remove(&device_id);
    println!("Device '{device_id}' disconnected.");
    publish_device_event(
        events.as_deref(),
        &device_id,
        "device_disconnected",
        serde_json::Value::Null,
    );
}

/// Blocks (polling) until the connection's first message arrives and is a
/// `Hello`. Anything else first, or a closed/erroring connection, ends the
/// attempt — a device must announce itself before doing anything else.
fn wait_for_hello<C: DeviceConnection>(wire: &Arc<Mutex<C>>) -> Option<DeviceMessage> {
    loop {
        match wire.lock().unwrap().poll() {
            Ok(DeviceEvent::Message(hello @ DeviceMessage::Hello { .. })) => return Some(hello),
            Ok(DeviceEvent::Message(_)) => return None,
            Ok(DeviceEvent::Idle) => thread::sleep(READ_POLL_INTERVAL),
            Ok(DeviceEvent::Closed) => return None,
            Err(_) => return None,
        }
    }
}

/// Pure handshake validation: unsupported protocol version is rejected
/// first (a version mismatch is worth reporting even to a device with a
/// bad token), then the token is checked. No token configured on Nala's
/// side means every device connection is rejected — fail closed, never
/// "accept everyone."
fn validate_hello(
    protocol_version: u16,
    token: &str,
    expected_token: Option<&str>,
) -> Result<(), RejectReason> {
    if protocol_version != PROTOCOL_VERSION {
        return Err(RejectReason::UnsupportedVersion);
    }

    match expected_token {
        Some(expected) if expected == token => Ok(()),
        _ => Err(RejectReason::BadToken),
    }
}

/// Publishes one `AutonomousEvent` built from a device's report -- either
/// a `DeviceMessage::Event` it sent, or a synthetic `device_connected` /
/// `device_disconnected` this listener generates itself. `events` is
/// `None` when no autonomous event queue is running (autonomous events
/// disabled, or an older caller); publishing is then a no-op rather than
/// a special case at every call site.
fn publish_device_event(
    events: Option<&dyn AutonomousEventQueue>,
    device_id: &str,
    kind: impl Into<String>,
    payload: serde_json::Value,
) {
    if let Some(events) = events {
        events.publish(AutonomousEvent::new(device_id, kind, payload));
    }
}

/// Binds `addr` and serves one device connection per accepted socket, each
/// on its own thread, forever. `token` is `None` when `NALA_DEVICE_TOKEN`
/// isn't set — every connection is then rejected (see `validate_hello`).
/// `events` is the autonomous event queue a connected device's reports
/// (and its connect/disconnect) are published to; `None` disables
/// autonomous events entirely for this listener.
pub fn serve(
    addr: &str,
    registry: Arc<DeviceRegistry<Device>>,
    token: Option<String>,
    events: Option<Arc<dyn AutonomousEventQueue>>,
) -> io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    println!("Nala listening for devices on ws://{addr}");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let registry = Arc::clone(&registry);
                let token = token.clone();
                let events = events.clone();
                thread::spawn(move || handle_connection(stream, registry, token, events));
            }
            Err(error) => eprintln!("Warning: failed to accept a device connection: {error}"),
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_matching_version_and_token_is_accepted() {
        assert_eq!(
            validate_hello(PROTOCOL_VERSION, "secret", Some("secret")),
            Ok(())
        );
    }

    #[test]
    fn a_hello_with_an_unsupported_protocol_version_is_rejected() {
        assert_eq!(
            validate_hello(PROTOCOL_VERSION + 1, "secret", Some("secret")),
            Err(RejectReason::UnsupportedVersion)
        );
    }

    #[test]
    fn a_hello_with_a_bad_token_is_rejected() {
        assert_eq!(
            validate_hello(PROTOCOL_VERSION, "wrong", Some("secret")),
            Err(RejectReason::BadToken)
        );
    }

    #[test]
    fn no_token_configured_rejects_every_device() {
        assert_eq!(
            validate_hello(PROTOCOL_VERSION, "anything", None),
            Err(RejectReason::BadToken)
        );
    }

    #[test]
    fn a_published_device_event_reaches_the_queue_tagged_with_its_device_id() {
        use crate::adapters::autonomous::in_memory_queue::InMemoryEventQueue;
        use crate::ports::autonomous::AutonomousEventQueue;

        let queue = InMemoryEventQueue::new(4);
        publish_device_event(
            Some(&queue),
            "esp32-bedroom",
            "battery_low",
            serde_json::json!({"percent": 9}),
        );

        let event = queue.next().expect("the event should have been published");
        assert_eq!(event.source, "esp32-bedroom");
        assert_eq!(event.kind, "battery_low");
        assert_eq!(event.payload, serde_json::json!({"percent": 9}));
    }

    #[test]
    fn publishing_with_no_queue_configured_is_a_no_op() {
        // No queue running (the local REPL has no device server at all,
        // and a device server can still start with autonomous events
        // disabled) must not panic.
        publish_device_event(None, "esp32-bedroom", "battery_low", serde_json::json!({}));
    }
}
