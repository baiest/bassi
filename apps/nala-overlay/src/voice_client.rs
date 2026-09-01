//! The connection to `voice --serve`'s audio WebSocket — the same protocol
//! `apps/android`'s `NalaSocket` speaks: send one binary WAV frame per
//! utterance, receive zero or more binary WAV clips back (narration, then
//! the final reply) in whatever order they arrive, no explicit
//! end-of-turn marker. Kept as a trait (rather than `tungstenite::WebSocket`
//! directly) so the read loop below is testable without a real socket —
//! same pattern as `nala::device_server::DeviceConnection`.

use std::io;

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

/// Polls `connection` until it closes or errors, handing every clip that
/// arrives to `on_clip` in the order it was received. Blocking — meant to
/// run on its own thread for the lifetime of one connection.
pub fn run_clip_loop<C: VoiceConnection>(connection: &mut C, mut on_clip: impl FnMut(Vec<u8>)) {
    loop {
        match connection.poll() {
            Ok(ClipEvent::Clip(bytes)) => on_clip(bytes),
            Ok(ClipEvent::Idle) => {}
            Ok(ClipEvent::Closed) => return,
            Err(_) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

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

    #[test]
    fn send_utterance_records_what_was_sent() {
        let mut connection = ScriptedConnection::new(vec![]);

        connection.send_utterance(b"hola".to_vec()).unwrap();

        assert_eq!(connection.sent, vec![b"hola".to_vec()]);
    }
}
