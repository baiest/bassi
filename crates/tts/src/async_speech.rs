use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;

use crate::speech::{Speech, SpeechError};

/// Max narration phrases allowed to sit in the queue before new ones are
/// dropped instead of enqueued. Narration is disposable: if a turn runs
/// long and the worker falls behind, the caller should catch up to what's
/// happening now rather than narrate minutes-old events in order.
const MAX_QUEUED_NARRATION: usize = 2;

enum Message {
    Speak(String),
    Flush(mpsc::Sender<()>),
}

/// Wraps any `Speech` backend to make it non-blocking. `say` and
/// `say_narration` return immediately; a single background worker thread
/// speaks queued text in order, so playback never overlaps and never gets
/// reordered relative to how it was enqueued.
///
/// Cloning shares the same queue (and the same worker) — this is what lets
/// an event sink and the final answer both speak through one `AsyncSpeech`
/// and have the answer come out after any in-flight narration, rather than
/// talking over it.
pub struct AsyncSpeech {
    sender: mpsc::Sender<Message>,
    pending: Arc<AtomicUsize>,
}

impl Clone for AsyncSpeech {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            pending: Arc::clone(&self.pending),
        }
    }
}

impl AsyncSpeech {
    pub fn new(backend: Box<dyn Speech + Send>) -> Self {
        let (sender, receiver) = mpsc::channel::<Message>();
        let pending = Arc::new(AtomicUsize::new(0));
        let worker_pending = Arc::clone(&pending);

        thread::spawn(move || {
            for message in receiver {
                match message {
                    Message::Speak(text) => {
                        let _ = backend.say(&text);
                        worker_pending.fetch_sub(1, Ordering::SeqCst);
                    }
                    Message::Flush(ack) => {
                        let _ = ack.send(());
                    }
                }
            }
        });

        Self { sender, pending }
    }

    /// Speaks `text` unless the queue is already backed up
    /// (`MAX_QUEUED_NARRATION` pending), in which case it's silently
    /// dropped. Never blocks. Meant for disposable narration, not the
    /// user-facing answer — use `Speech::say` for that.
    pub fn say_narration(&self, text: &str) {
        if self.pending.load(Ordering::SeqCst) >= MAX_QUEUED_NARRATION {
            return;
        }
        self.enqueue(text);
    }

    /// Whether anything is queued or currently being spoken.
    ///
    /// The always-listening pipeline (BAS-25) gates the microphone on
    /// this: reading audio while Nala is speaking would let her hear her
    /// own voice and wake herself. It drops to `false` the instant the
    /// last message finishes, before the acoustic tail has actually left
    /// the speaker, so callers still need their own short settle window
    /// after this clears — this only rules out the far larger risk of
    /// reading the microphone throughout an entire turn's narration.
    pub fn is_speaking(&self) -> bool {
        self.pending.load(Ordering::SeqCst) > 0
    }

    /// Blocks until every message enqueued so far has been spoken.
    pub fn flush(&self) {
        let (ack_tx, ack_rx) = mpsc::channel();
        if self.sender.send(Message::Flush(ack_tx)).is_ok() {
            let _ = ack_rx.recv();
        }
    }

    fn enqueue(&self, text: &str) {
        self.pending.fetch_add(1, Ordering::SeqCst);
        if self.sender.send(Message::Speak(text.to_string())).is_err() {
            // Worker thread is gone; undo the optimistic increment so
            // `pending` doesn't grow unbounded.
            self.pending.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

impl Speech for AsyncSpeech {
    /// Always enqueues — never dropped, unlike `say_narration`. The
    /// user-facing answer must always be heard.
    fn say(&self, text: &str) -> Result<(), SpeechError> {
        self.enqueue(text);
        Ok(())
    }
}
