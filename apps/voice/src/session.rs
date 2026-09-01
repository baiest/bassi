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
            narrator: Mutex::new(narrator),
            synth: Mutex::new(synth),
            outbox: Outbox::new(),
        }
    }

    pub fn outbox(&self) -> &Outbox {
        &self.outbox
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
        let mut client_slot = self.client.lock().unwrap();
        if client_slot.is_none() {
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

            // Nala is the one greeting — synthesize it the same way as any
            // other reply and queue it, so whichever phone connects next
            // hears it. Best-effort: a connection that can't produce a
            // greeting still proceeds to serve turns.
            match client.recv_greeting() {
                Ok(greeting) if !greeting.is_empty() => {
                    match synthesize_to_wav(self.synth.lock().unwrap().as_ref(), &greeting) {
                        Ok(clip) => self.outbox.push(clip),
                        Err(error) => {
                            eprintln!("Warning: could not synthesize the greeting: {error}")
                        }
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    eprintln!("Warning: could not receive the greeting from nala: {error}")
                }
            }

            *client_slot = Some(client);
        }
        let client = client_slot.as_mut().expect("just ensured connected");

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
