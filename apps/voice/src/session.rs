//! A single persistent session shared by every phone connection: one
//! connection to Nala, reused across phone reconnects, and an outbox of
//! synthesized audio clips waiting to be delivered. A turn keeps running
//! and queuing its output even if the phone drops mid-turn — the next
//! phone connection (the same one, reconnected, or a new one) picks up
//! delivery where it left off instead of the reply being lost.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use agent_protocol::Event;
use tts::StreamSynthesizeSpeech;

use crate::audio_server::synthesize_to_wav;
use crate::client::{NalaClient, TcpWire};
use crate::narrator::Narrator;

/// How long an undelivered clip is worth keeping. Past this, playing it
/// back would mean speaking about something from a while ago with no
/// context — better to drop it than confuse whoever reconnects.
const TTL: Duration = Duration::from_secs(5 * 60);
/// Caps memory if a phone stays disconnected for a long time; once a turn
/// (or several) produces more clips than this, the oldest are dropped
/// first.
const MAX_CLIPS: usize = 32;

/// A FIFO queue of pending audio clips, safe to push from a turn's worker
/// thread while a connection's own thread pops from it.
#[derive(Default)]
pub struct Outbox {
    clips: Mutex<VecDeque<(Instant, Vec<u8>)>>,
    ready: Condvar,
}

impl Outbox {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues `clip`, purging anything past `TTL` and trimming down to
    /// `MAX_CLIPS` from the front (oldest first) so a long-disconnected
    /// phone doesn't build up an unbounded backlog.
    pub fn push(&self, clip: Vec<u8>) {
        let mut clips = self.clips.lock().unwrap();
        purge_expired(&mut clips);
        clips.push_back((Instant::now(), clip));
        while clips.len() > MAX_CLIPS {
            clips.pop_front();
        }
        self.ready.notify_all();
    }

    /// Puts `clip` back at the front of the queue, restarting its TTL —
    /// used when delivery failed partway (the connection died mid-send),
    /// so the clip isn't lost, just retried on the next connection.
    pub fn push_front(&self, clip: Vec<u8>) {
        let mut clips = self.clips.lock().unwrap();
        clips.push_front((Instant::now(), clip));
        self.ready.notify_all();
    }

    /// Returns the next clip without waiting, or `None` if the queue is
    /// currently empty.
    pub fn try_pop(&self) -> Option<Vec<u8>> {
        let mut clips = self.clips.lock().unwrap();
        purge_expired(&mut clips);
        clips.pop_front().map(|(_, clip)| clip)
    }

    /// Blocks up to `timeout` for a clip, discarding any that expired
    /// while waiting. Returns `None` on timeout.
    pub fn pop_blocking(&self, timeout: Duration) -> Option<Vec<u8>> {
        let mut clips = self.clips.lock().unwrap();
        let deadline = Instant::now() + timeout;
        loop {
            purge_expired(&mut clips);
            if let Some((_, clip)) = clips.pop_front() {
                return Some(clip);
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            let (guard, _) = self.ready.wait_timeout(clips, deadline - now).unwrap();
            clips = guard;
        }
    }
}

fn purge_expired(clips: &mut VecDeque<(Instant, Vec<u8>)>) {
    let now = Instant::now();
    while let Some((queued_at, _)) = clips.front() {
        if now.duration_since(*queued_at) > TTL {
            clips.pop_front();
        } else {
            break;
        }
    }
}

/// The one long-lived session an audio server process runs: a connection to
/// Nala (reconnected lazily when it dies), a narrator, a TTS backend, and
/// the outbox turns write their audio into. Shared across every phone
/// connection via `Arc`, so a phone reconnecting attaches to the same
/// in-flight state rather than starting over.
pub struct VoiceSession {
    nala_addr: String,
    client: Mutex<Option<NalaClient<TcpWire>>>,
    // Nala sends its greeting only once, right when *its* connection opens
    // — not once per phone/PC that connects to us. Cached here so every
    // client that connects to this process can still get its own freshly
    // synthesized clip (see `greeting_clip`), instead of only whichever
    // client happened to be first.
    greeting: Mutex<Option<String>>,
    narrator: Mutex<Box<dyn Narrator + Send>>,
    synth: Mutex<Box<dyn StreamSynthesizeSpeech + Send>>,
    outbox: Outbox,
}

impl VoiceSession {
    pub fn new(
        nala_addr: String,
        narrator: Box<dyn Narrator + Send>,
        synth: Box<dyn StreamSynthesizeSpeech + Send>,
    ) -> Self {
        Self {
            nala_addr,
            client: Mutex::new(None),
            greeting: Mutex::new(None),
            narrator: Mutex::new(narrator),
            synth: Mutex::new(synth),
            outbox: Outbox::new(),
        }
    }

    pub fn outbox(&self) -> &Outbox {
        &self.outbox
    }

    /// Connects to Nala if there's no live connection yet, caching
    /// whatever greeting it sends (see `greeting_clip`). Idempotent: a
    /// call that finds a connection already up does nothing — Nala only
    /// ever sends its greeting once, right when its own connection opens.
    pub fn ensure_connected(&self) {
        let mut client_slot = self.client.lock().unwrap();
        if client_slot.is_some() {
            return;
        }

        let mut client = match TcpWire::connect(&self.nala_addr) {
            Ok(wire) => NalaClient::new(wire),
            Err(error) => {
                eprintln!(
                    "Error: could not connect to nala at {}: {error}",
                    self.nala_addr
                );
                return;
            }
        };

        // Best-effort: a connection that can't produce a greeting still
        // proceeds to serve turns.
        match client.recv_greeting() {
            Ok(greeting) if !greeting.is_empty() => {
                *self.greeting.lock().unwrap() = Some(greeting);
            }
            Ok(_) => {}
            Err(error) => {
                eprintln!("Warning: could not receive the greeting from nala: {error}")
            }
        }

        *client_slot = Some(client);
    }

    /// Connects to Nala if needed, then synthesizes a fresh greeting clip
    /// for *this* caller from the cached text — meant to be called once
    /// per client connection (see `audio_server::handle_connection`) and
    /// sent directly on that connection's own wire, not through the
    /// shared `outbox`: the outbox has no notion of "which client asked
    /// for this," so two clients connected at once could otherwise steal
    /// each other's greeting. `None` if Nala gave no greeting, or the
    /// connection couldn't be made at all.
    pub fn greeting_clip(&self) -> Option<Vec<u8>> {
        self.ensure_connected();
        let greeting = self.greeting.lock().unwrap().clone()?;
        match synthesize_to_wav(self.synth.lock().unwrap().as_ref(), &greeting) {
            Ok(clip) => Some(clip),
            Err(error) => {
                eprintln!("Warning: could not synthesize the greeting: {error}");
                None
            }
        }
    }

    /// Runs `text` as one turn on its own thread, so a phone connection
    /// dropping mid-turn doesn't cut the turn short — it keeps running and
    /// queues its clips to the outbox regardless of who's listening.
    pub fn submit(self: &Arc<Self>, text: String) {
        let session = Arc::clone(self);
        thread::spawn(move || session.run_turn(&text));
    }

    /// Reconnects to Nala if the last connection died, sends `text`, and
    /// queues every resulting clip (narration as it arrives, then the
    /// final reply) to the outbox. Holding `client`'s lock for the whole
    /// turn serializes turns against each other, which is what we want:
    /// one `Assistant` on the Nala side, one turn at a time.
    fn run_turn(&self, text: &str) {
        self.ensure_connected();

        let mut client_slot = self.client.lock().unwrap();
        let Some(client) = client_slot.as_mut() else {
            // `ensure_connected` already logged why.
            return;
        };

        let outbox = &self.outbox;
        let narrator = &self.narrator;
        let synth = &self.synth;
        let reply = client.send(text, |event: Event| {
            let phrase = narrator.lock().unwrap().narrate(&event);
            let Some(phrase) = phrase else { return };
            let synthesized = synthesize_to_wav(synth.lock().unwrap().as_ref(), &phrase);
            match synthesized {
                Ok(clip) => outbox.push(clip),
                Err(error) => eprintln!("Warning: could not synthesize narration: {error}"),
            }
        });

        match reply {
            Ok(text) => {
                let synthesized = synthesize_to_wav(synth.lock().unwrap().as_ref(), &text);
                match synthesized {
                    Ok(clip) => outbox.push(clip),
                    Err(error) => eprintln!("Warning: could not synthesize the reply: {error}"),
                }
            }
            Err(error) => {
                eprintln!("Error: {error}");
                // The Nala connection is presumably dead; drop it so the
                // next turn reconnects instead of failing the same way
                // forever (`TcpWire::connect` is one-shot, no retry).
                *client_slot = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::mpsc;

    use agent_protocol::ServerMessage;
    use tts::{PcmStream, SpeechError};
    use tungstenite::Message;

    struct FakeSynth;

    impl StreamSynthesizeSpeech for FakeSynth {
        fn synthesize_stream(&self, text: &str) -> Result<PcmStream, SpeechError> {
            let (tx, rx) = mpsc::channel();
            let samples: Vec<i16> = (0..text.len() as i16).collect();
            tx.send(Ok(samples)).unwrap();
            Ok(PcmStream {
                sample_rate: 16_000,
                channels: 1,
                chunks: rx,
            })
        }
    }

    struct SilentNarrator;
    impl Narrator for SilentNarrator {
        fn narrate(&mut self, _event: &Event) -> Option<String> {
            None
        }
    }

    /// Spawns a fake Nala that sends only its greeting and then goes
    /// quiet, and returns the address to connect to.
    fn spawn_fake_nala_greeting_only(greeting: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port");
        let addr = listener.local_addr().expect("local addr").to_string();

        thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept a connection from voice");
            let mut ws = tungstenite::accept(stream).expect("complete the WS handshake");
            let json = serde_json::to_string(&ServerMessage::Event(Event::Greeting {
                text: greeting.to_string(),
            }))
            .unwrap();
            ws.send(Message::Text(json)).expect("send the greeting");
            // Keep the connection open so the client isn't surprised by an
            // immediate close.
            thread::sleep(Duration::from_secs(5));
        });

        addr
    }

    #[test]
    fn greeting_clip_returns_a_clip_synthesized_from_nalas_greeting() {
        let nala_addr = spawn_fake_nala_greeting_only("hola");
        let session = VoiceSession::new(nala_addr, Box::new(SilentNarrator), Box::new(FakeSynth));

        let clip = session
            .greeting_clip()
            .expect("a greeting clip should be produced");

        assert_eq!(clip, wav_of_len("hola".len()));
    }

    #[test]
    fn greeting_clip_never_touches_the_shared_outbox() {
        let nala_addr = spawn_fake_nala_greeting_only("hola");
        let session = VoiceSession::new(nala_addr, Box::new(SilentNarrator), Box::new(FakeSynth));

        session.greeting_clip();

        // The greeting must be handed straight back to the caller, not
        // queued where some other connection could pop it instead.
        assert!(session.outbox().try_pop().is_none());
    }

    #[test]
    fn every_connecting_client_gets_its_own_greeting_clip() {
        let nala_addr = spawn_fake_nala_greeting_only("hola");
        let session = VoiceSession::new(nala_addr, Box::new(SilentNarrator), Box::new(FakeSynth));

        // Simulates two separate clients (e.g. Android and nala-overlay)
        // each connecting and asking for their own greeting — Nala itself
        // is only contacted once (`spawn_fake_nala_greeting_only` accepts
        // a single connection), but each caller still gets a clip.
        let first = session.greeting_clip();
        let second = session.greeting_clip();

        assert_eq!(first, Some(wav_of_len("hola".len())));
        assert_eq!(second, Some(wav_of_len("hola".len())));
    }

    #[test]
    fn ensure_connected_is_a_no_op_once_already_connected() {
        let nala_addr = spawn_fake_nala_greeting_only("hola");
        let session = VoiceSession::new(nala_addr, Box::new(SilentNarrator), Box::new(FakeSynth));

        session.ensure_connected();
        let cached_after_first = session.greeting.lock().unwrap().clone();
        session.ensure_connected();

        // A reconnect (or a lost greeting) would show up as the cached
        // text changing or disappearing.
        assert_eq!(session.greeting.lock().unwrap().clone(), cached_after_first);
        assert_eq!(cached_after_first, Some("hola".to_string()));
    }

    fn wav_of_len(sample_count: usize) -> Vec<u8> {
        let samples: Vec<i16> = (0..sample_count as i16).collect();
        crate::wav::encode_wav(&samples, 16_000, 1)
    }

    #[test]
    fn pops_clips_in_fifo_order() {
        let outbox = Outbox::new();
        outbox.push(b"one".to_vec());
        outbox.push(b"two".to_vec());
        assert_eq!(
            outbox.pop_blocking(Duration::from_millis(10)),
            Some(b"one".to_vec())
        );
        assert_eq!(
            outbox.pop_blocking(Duration::from_millis(10)),
            Some(b"two".to_vec())
        );
    }

    #[test]
    fn pop_blocking_times_out_when_empty() {
        let outbox = Outbox::new();
        assert_eq!(outbox.pop_blocking(Duration::from_millis(10)), None);
    }

    #[test]
    fn trims_to_max_clips_dropping_oldest_first() {
        let outbox = Outbox::new();
        for i in 0..(MAX_CLIPS + 5) {
            outbox.push(vec![i as u8]);
        }
        let mut popped = Vec::new();
        while let Some(clip) = outbox.try_pop() {
            popped.push(clip[0]);
        }
        assert_eq!(popped.len(), MAX_CLIPS);
        // The 5 oldest (0..5) were dropped, so delivery starts at 5.
        assert_eq!(popped.first(), Some(&5u8));
    }

    #[test]
    fn push_front_is_delivered_before_later_pushes() {
        let outbox = Outbox::new();
        outbox.push(b"second".to_vec());
        outbox.push_front(b"first".to_vec());
        assert_eq!(outbox.try_pop(), Some(b"first".to_vec()));
        assert_eq!(outbox.try_pop(), Some(b"second".to_vec()));
    }

    #[test]
    fn expired_clips_are_dropped_instead_of_delivered() {
        let outbox = Outbox::new();
        // Simulate age by pushing directly into the internal queue rather
        // than sleeping five minutes in a test.
        {
            let mut clips = outbox.clips.lock().unwrap();
            clips.push_back((
                Instant::now() - TTL - Duration::from_secs(1),
                b"stale".to_vec(),
            ));
        }
        outbox.push(b"fresh".to_vec());
        assert_eq!(outbox.try_pop(), Some(b"fresh".to_vec()));
    }
}
