use std::io;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

use device_protocol::DeviceState;
use tungstenite::Message;

/// Broadcasts `DeviceState` to every locally-connected overlay. Pure
/// publish/subscribe logic, kept separate from the WebSocket plumbing in
/// `serve` below so it's testable without a socket: the daemon calls
/// `set_state` around each `Invoke`, and an overlay (or a test) calls
/// `subscribe` to receive the current state immediately, then every
/// change after that.
pub struct OverlayChannel {
    state: Mutex<DeviceState>,
    subscribers: Mutex<Vec<mpsc::Sender<DeviceState>>>,
}

impl OverlayChannel {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(DeviceState::Idle),
            subscribers: Mutex::new(Vec::new()),
        }
    }

    pub fn set_state(&self, state: DeviceState) {
        *self.state.lock().unwrap() = state;
        // A subscriber whose receiver was dropped (its connection closed,
        // or in tests, the handle was just discarded) fails to send —
        // dropped from the list rather than treated as an error, so one
        // gone overlay never affects the daemon or any other subscriber.
        self.subscribers
            .lock()
            .unwrap()
            .retain(|sender| sender.send(state).is_ok());
    }

    /// Registers a new subscriber and immediately queues the current state
    /// on it, so a connecting overlay renders the right thing on its very
    /// first frame instead of waiting for the next transition.
    pub fn subscribe(&self) -> mpsc::Receiver<DeviceState> {
        let (sender, receiver) = mpsc::channel();
        let current = *self.state.lock().unwrap();
        let _ = sender.send(current);
        self.subscribers.lock().unwrap().push(sender);
        receiver
    }
}

impl Default for OverlayChannel {
    fn default() -> Self {
        Self::new()
    }
}

/// Serves `channel`'s state to any local subscriber connecting to `addr`,
/// one WebSocket per subscriber, forever. Deliberately thin: all the
/// interesting logic (fan-out, current-state-on-connect, a dead
/// subscriber never breaking anything) lives in `OverlayChannel` above,
/// covered by its own unit tests.
pub fn serve(addr: &str, channel: Arc<OverlayChannel>) -> io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    println!("Nala PC daemon overlay listening on ws://{addr}");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let channel = Arc::clone(&channel);
                thread::spawn(move || handle_subscriber(stream, channel));
            }
            Err(error) => eprintln!("Warning: failed to accept an overlay connection: {error}"),
        }
    }

    Ok(())
}

fn handle_subscriber(stream: TcpStream, channel: Arc<OverlayChannel>) {
    let mut ws = match tungstenite::accept(stream) {
        Ok(ws) => ws,
        Err(error) => {
            eprintln!("Warning: overlay WebSocket handshake failed: {error}");
            return;
        }
    };

    let receiver = channel.subscribe();
    for state in receiver {
        let Ok(json) = serde_json::to_string(&state) else {
            continue;
        };
        if ws.send(Message::Text(json)).is_err() {
            // The overlay closed its end — nothing more to do here; the
            // `OverlayChannel` cleans this subscriber up the next time it
            // tries to deliver a state and the send fails.
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_subscriber_receives_the_current_state_when_it_connects() {
        let channel = OverlayChannel::new();
        channel.set_state(DeviceState::Executing);

        let receiver = channel.subscribe();

        assert_eq!(receiver.recv().unwrap(), DeviceState::Executing);
    }

    #[test]
    fn a_subscriber_then_receives_every_subsequent_state_change() {
        let channel = OverlayChannel::new();
        let receiver = channel.subscribe();
        assert_eq!(receiver.recv().unwrap(), DeviceState::Idle);

        channel.set_state(DeviceState::Executing);
        channel.set_state(DeviceState::Idle);

        assert_eq!(receiver.recv().unwrap(), DeviceState::Executing);
        assert_eq!(receiver.recv().unwrap(), DeviceState::Idle);
    }

    #[test]
    fn a_dead_subscriber_does_not_break_the_daemon() {
        let channel = OverlayChannel::new();
        let dropped = channel.subscribe();
        drop(dropped);
        let alive = channel.subscribe();

        // Must not panic, and the still-alive subscriber must still get
        // the update even though the dropped one's send would fail.
        channel.set_state(DeviceState::Error);

        assert_eq!(alive.recv().unwrap(), DeviceState::Idle);
        assert_eq!(alive.recv().unwrap(), DeviceState::Error);
    }
}
