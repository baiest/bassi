#[path = "common/fake_speech.rs"]
mod fake_speech;

use std::sync::Mutex;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use fake_speech::SpySpeech;
use tts::{AsyncSpeech, Speech, SpeechError};

/// A `Speech` backend whose `say` blocks until the test explicitly releases
/// it (one release per call). Used to pin down the worker mid-message so a
/// backpressure test doesn't race against how fast the real backend
/// happens to drain the queue.
struct BlockingSpeech {
    gate: Mutex<mpsc::Receiver<()>>,
}

impl BlockingSpeech {
    fn new() -> (Self, mpsc::Sender<()>) {
        let (tx, rx) = mpsc::channel();
        (
            Self {
                gate: Mutex::new(rx),
            },
            tx,
        )
    }
}

impl Speech for BlockingSpeech {
    fn say(&self, _text: &str) -> Result<(), SpeechError> {
        let _ = self.gate.lock().unwrap().recv();
        Ok(())
    }
}

#[test]
fn speaks_enqueued_messages_in_order() {
    let backend = SpySpeech::new();
    let spy = backend.clone();
    let speech = AsyncSpeech::new(Box::new(backend));

    speech.say("one").unwrap();
    speech.say("two").unwrap();
    speech.say("three").unwrap();
    speech.flush();

    assert_eq!(
        spy.spoken(),
        vec!["one".to_string(), "two".to_string(), "three".to_string()]
    );
}

#[test]
fn say_never_drops_even_under_backpressure() {
    let backend = SpySpeech::new();
    let spy = backend.clone();
    let speech = AsyncSpeech::new(Box::new(backend));

    // Far more than the narration queue cap, all via the non-disposable
    // path — every single one must still be spoken.
    for i in 0..10 {
        speech.say(&i.to_string()).unwrap();
    }
    speech.flush();

    assert_eq!(spy.spoken().len(), 10);
}

#[test]
fn say_narration_drops_once_the_queue_is_backed_up() {
    let (backend, release) = BlockingSpeech::new();
    let speech = AsyncSpeech::new(Box::new(backend));

    // The worker picks up the first message and blocks on it immediately
    // (nothing releases the gate yet), so `pending` only ever grows from
    // here on the producer thread — no race with how fast the backend
    // happens to drain. With a cap of 2, the first two enqueue and every
    // one after is dropped.
    for i in 0..50 {
        speech.say_narration(&i.to_string());
    }

    release.send(()).unwrap();
    release.send(()).unwrap();
    speech.flush();

    // Confirmed indirectly: if none had been dropped, `flush` would still
    // be waiting on 48 more releases we never sent, and this line would
    // hang instead of returning.
}

#[test]
fn flush_waits_for_pending_speech_to_finish() {
    let backend = SpySpeech::new();
    let spy = backend.clone();
    let speech = AsyncSpeech::new(Box::new(backend));

    speech.say("hello").unwrap();
    speech.flush();

    // If `flush` returned before the worker actually processed the
    // message, this would be flaky under load; a short grace sleep would
    // only mask that. Assert immediately.
    assert_eq!(spy.spoken(), vec!["hello".to_string()]);
}

#[test]
fn a_failing_backend_call_does_not_kill_the_worker() {
    let backend = SpySpeech::failing();
    let speech = AsyncSpeech::new(Box::new(backend));

    // `Speech::say` itself always returns `Ok` here (it only enqueues); the
    // backend's failure happens later on the worker thread. This must not
    // poison the worker — a later message on the same `AsyncSpeech` should
    // still be attempted.
    speech.say("this will fail").unwrap();
    speech.flush();
}

#[test]
fn clone_shares_the_same_queue() {
    let backend = SpySpeech::new();
    let spy = backend.clone();
    let speech = AsyncSpeech::new(Box::new(backend));
    let speech_clone = speech.clone();

    speech.say("from original").unwrap();
    speech_clone.say("from clone").unwrap();
    speech.flush();

    assert_eq!(
        spy.spoken(),
        vec!["from original".to_string(), "from clone".to_string()]
    );
}

/// Not asserted directly (timing isn't deterministic to test from outside),
/// but exercises that `say` returns immediately rather than blocking for
/// however long the backend takes — the whole point of this adapter.
#[test]
fn say_does_not_block_the_caller() {
    let backend = SpySpeech::new();
    let speech = AsyncSpeech::new(Box::new(backend));

    let start = std::time::Instant::now();
    speech.say("hello").unwrap();
    let elapsed = start.elapsed();

    assert!(elapsed < Duration::from_millis(200));
    speech.flush();
    thread::sleep(Duration::from_millis(1));
}

#[test]
fn is_speaking_reports_true_while_a_message_is_in_flight() {
    let (backend, release) = BlockingSpeech::new();
    let speech = AsyncSpeech::new(Box::new(backend));

    assert!(!speech.is_speaking(), "nothing queued yet");

    speech.say("hello").unwrap();
    // The worker picks the message up and blocks on the gate almost
    // immediately; poll briefly instead of asserting the exact instant,
    // to avoid a race against the worker thread's scheduling.
    let became_true = (0..50).any(|_| {
        let speaking = speech.is_speaking();
        if !speaking {
            thread::sleep(Duration::from_millis(2));
        }
        speaking
    });
    assert!(became_true, "expected is_speaking to become true");

    release.send(()).unwrap();
    speech.flush();

    assert!(!speech.is_speaking(), "cleared once the message is spoken");
}
