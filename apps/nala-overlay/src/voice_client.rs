//! The connection to `voice --serve`'s audio WebSocket — the same protocol
//! `apps/android`'s `NalaSocket` speaks: send one binary WAV frame per
//! utterance, receive zero or more binary WAV clips back (narration, then
//! the final reply) in whatever order they arrive, no explicit
//! end-of-turn marker. Kept as a trait (rather than `tungstenite::WebSocket`
//! directly) so the read loop below is testable without a real socket —
//! same pattern as `nala::device_server::DeviceConnection`.

use std::io;
use std::sync::{Arc, Mutex};

/// What one poll of the connection produced.
pub enum ClipEvent {
    Clip(Vec<u8>),
    /// Nothing arrived within the read timeout — not an error, just a
    /// chance for the caller to check anything else it needs to (e.g. a
    /// pending utterance to send) and poll again.
    Idle,
    Closed,
}

pub trait VoiceConnection {
    fn poll(&mut self) -> io::Result<ClipEvent>;
    fn send_utterance(&mut self, wav: Vec<u8>) -> io::Result<()>;
}

impl<S: std::io::Read + std::io::Write> VoiceConnection for tungstenite::WebSocket<S> {
    fn poll(&mut self) -> io::Result<ClipEvent> {
        match self.read() {
            Ok(tungstenite::Message::Binary(bytes)) => Ok(ClipEvent::Clip(bytes)),
            Ok(tungstenite::Message::Close(_)) => Ok(ClipEvent::Closed),
            // Text/ping/pong noise: this protocol is audio-only, same as
            // `voice::audio_server`'s own wire.
            Ok(_) => Ok(ClipEvent::Idle),
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                Ok(ClipEvent::Closed)
            }
            Err(tungstenite::Error::Io(ref error))
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                Ok(ClipEvent::Idle)
            }
            Err(error) => Err(io::Error::other(error)),
        }
    }

    fn send_utterance(&mut self, wav: Vec<u8>) -> io::Result<()> {
        tungstenite::WebSocket::send(self, tungstenite::Message::Binary(wav))
            .map_err(io::Error::other)
    }
}

/// Locks only for the duration of each call, never across one — so a
/// `run_clip_loop` driven through a shared connection releases the lock
/// between every poll, instead of holding it for an entire blocking read
/// and starving whoever else wants to send on the same connection (e.g. an
/// utterance going out while this loop is mid-read).
impl<T: VoiceConnection> VoiceConnection for Arc<Mutex<T>> {
    fn poll(&mut self) -> io::Result<ClipEvent> {
        self.lock().unwrap().poll()
    }

    fn send_utterance(&mut self, wav: Vec<u8>) -> io::Result<()> {
        self.lock().unwrap().send_utterance(wav)
    }
}

/// Polls `connection` until it closes or errors, handing every clip that
/// arrives to `on_clip` in the order it was received. Blocking — meant to
/// run on its own thread for the lifetime of one connection.
///
/// The brief sleep after an `Idle` poll matters when `connection` is a
/// lock-per-call wrapper like `Arc<Mutex<T>>` (see its `VoiceConnection`
/// impl above): a std `Mutex` isn't fair, so a driver that unlocks and
/// immediately relocks in a tight loop can starve another thread trying to
/// acquire the same lock (e.g. to send an utterance) — this widens the gap
/// enough for the scheduler to actually hand it over.
pub fn run_clip_loop<C: VoiceConnection>(connection: &mut C, mut on_clip: impl FnMut(Vec<u8>)) {
    loop {
        match connection.poll() {
            Ok(ClipEvent::Clip(bytes)) => on_clip(bytes),
            Ok(ClipEvent::Idle) => std::thread::sleep(std::time::Duration::from_millis(1)),
            Ok(ClipEvent::Closed) => return,
            Err(_) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::time::Duration;

    struct ScriptedConnection {
        incoming: VecDeque<ClipEvent>,
        sent: Vec<Vec<u8>>,
    }

    impl ScriptedConnection {
        fn new(incoming: Vec<ClipEvent>) -> Self {
            Self {
                incoming: incoming.into(),
                sent: Vec::new(),
            }
        }
    }

    impl VoiceConnection for ScriptedConnection {
        fn poll(&mut self) -> io::Result<ClipEvent> {
            Ok(self.incoming.pop_front().unwrap_or(ClipEvent::Closed))
        }

        fn send_utterance(&mut self, wav: Vec<u8>) -> io::Result<()> {
            self.sent.push(wav);
            Ok(())
        }
    }

    #[test]
    fn every_clip_is_delivered_in_order() {
        let mut connection = ScriptedConnection::new(vec![
            ClipEvent::Clip(b"one".to_vec()),
            ClipEvent::Idle,
            ClipEvent::Clip(b"two".to_vec()),
            ClipEvent::Closed,
        ]);

        let mut received = Vec::new();
        run_clip_loop(&mut connection, |clip| received.push(clip));

        assert_eq!(received, vec![b"one".to_vec(), b"two".to_vec()]);
    }

    #[test]
    fn the_loop_ends_when_the_connection_closes() {
        let mut connection = ScriptedConnection::new(vec![ClipEvent::Closed]);

        let mut received = Vec::new();
        run_clip_loop(&mut connection, |clip| received.push(clip));

        assert!(received.is_empty());
    }

    /// A connection whose `poll()` takes a moment each call (like a real
    /// blocking socket read would) before reporting `Idle`, so a test can
    /// tell the difference between "the lock is released between polls"
    /// and "the lock is held for the whole loop".
    struct SlowConnection {
        remaining_polls: usize,
        sent: Vec<Vec<u8>>,
    }

    impl VoiceConnection for SlowConnection {
        fn poll(&mut self) -> io::Result<ClipEvent> {
            std::thread::sleep(Duration::from_millis(20));
            if self.remaining_polls == 0 {
                return Ok(ClipEvent::Closed);
            }
            self.remaining_polls -= 1;
            Ok(ClipEvent::Idle)
        }

        fn send_utterance(&mut self, wav: Vec<u8>) -> io::Result<()> {
            self.sent.push(wav);
            Ok(())
        }
    }

    #[test]
    fn a_mutex_wrapped_connection_releases_the_lock_between_polls() {
        use std::thread;
        use std::time::Instant;

        // 20 polls * 20ms/poll = ~400ms of total loop time — long enough
        // that "the lock is held for the whole loop" and "the lock is
        // released between polls" are clearly distinguishable by timing.
        let connection = Arc::new(Mutex::new(SlowConnection {
            remaining_polls: 20,
            sent: Vec::new(),
        }));
        let mut driver = Arc::clone(&connection);

        let loop_thread = thread::spawn(move || {
            run_clip_loop(&mut driver, |_clip| {});
        });

        // Let the loop get going, then send while it's still mid-flight.
        thread::sleep(Duration::from_millis(50));
        let mut sender = Arc::clone(&connection);
        let started = Instant::now();
        sender.send_utterance(b"hola".to_vec()).unwrap();
        let elapsed = started.elapsed();

        loop_thread.join().unwrap();

        // If the lock were held for the whole loop, this send would have
        // waited out the remaining ~300ms of polling instead of getting in
        // during a gap between two individual polls. Generous threshold to
        // absorb scheduling jitter while still being well short of that.
        assert!(
            elapsed < Duration::from_millis(300),
            "send_utterance took {elapsed:?}, the lock looks held across the whole loop"
        );
        assert_eq!(connection.lock().unwrap().sent, vec![b"hola".to_vec()]);
    }

    #[test]
    fn send_utterance_records_what_was_sent() {
        let mut connection = ScriptedConnection::new(vec![]);

        connection.send_utterance(b"hola".to_vec()).unwrap();

        assert_eq!(connection.sent, vec![b"hola".to_vec()]);
    }
}
