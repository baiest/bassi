#[path = "common/fake_events.rs"]
mod fake_events;

#[path = "common/fake_speech.rs"]
mod fake_speech;

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::mpsc;

use agent_protocol::{Event, EventSink, LlmCallId, TaskId};
use fake_events::RecordingEventSink;
use fake_speech::SpySpeech;
use tts::{AsyncSpeech, Speech, SpeechError};
use voice::narrator::Narrator;
use voice::speaking_sink::SpeakingEventSink;

/// A narrator whose answer is scripted per call, so tests can drive
/// `SpeakingEventSink` without depending on `TemplateNarrator`'s actual
/// phrase bank.
struct ScriptedNarrator {
    answers: VecDeque<Option<String>>,
}

impl ScriptedNarrator {
    fn new(answers: Vec<Option<&str>>) -> Self {
        Self {
            answers: answers.into_iter().map(|a| a.map(str::to_string)).collect(),
        }
    }
}

impl Narrator for ScriptedNarrator {
    fn narrate(&mut self, _event: &Event) -> Option<String> {
        self.answers.pop_front().flatten()
    }
}

/// See the identically-named helper in `crates/tts/tests/async_speech.rs`
/// for why a blocking backend (not `SpySpeech`) is needed to test dropping
/// under backpressure deterministically.
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
fn forwards_every_event_to_the_inner_sink_unchanged() {
    let recorder = RecordingEventSink::new();
    let narrator = ScriptedNarrator::new(vec![None, None]);
    let backend = SpySpeech::new();
    let speech = AsyncSpeech::new(Box::new(backend));

    let mut sink = SpeakingEventSink::new(recorder, narrator, speech);

    sink.emit(Event::RequestStarted {
        task_id: TaskId::new(),
    });
    sink.emit(Event::Cancelled {
        task_id: TaskId::new(),
    });

    assert_eq!(sink.inner().events.len(), 2);
    assert!(matches!(
        sink.inner().events[0],
        Event::RequestStarted { .. }
    ));
    assert!(matches!(sink.inner().events[1], Event::Cancelled { .. }));
}

#[test]
fn speaks_the_phrase_the_narrator_returns() {
    let recorder = RecordingEventSink::new();
    let narrator = ScriptedNarrator::new(vec![Some("hola")]);
    let backend = SpySpeech::new();
    let spy = backend.clone();
    let speech = AsyncSpeech::new(Box::new(backend));

    let mut sink = SpeakingEventSink::new(recorder, narrator, speech.clone());
    sink.emit(Event::RequestStarted {
        task_id: TaskId::new(),
    });
    speech.flush();

    assert_eq!(spy.spoken(), vec!["hola".to_string()]);
}

#[test]
fn stays_silent_when_the_narrator_has_nothing_to_say() {
    let recorder = RecordingEventSink::new();
    let narrator = ScriptedNarrator::new(vec![None]);
    let backend = SpySpeech::new();
    let spy = backend.clone();
    let speech = AsyncSpeech::new(Box::new(backend));

    let mut sink = SpeakingEventSink::new(recorder, narrator, speech.clone());
    let task_id = TaskId::new();
    sink.emit(Event::LlmStarted {
        llm_call_id: LlmCallId::new(&task_id, 1),
        task_id,
        call_index: 1,
        images: 0,
    });
    speech.flush();

    assert!(spy.spoken().is_empty());
}

#[test]
fn narration_uses_the_disposable_path_not_the_never_drop_one() {
    // Regression guard: `SpeakingEventSink` must call `say_narration`
    // (drops under backpressure), not `Speech::say` (never drops) —
    // otherwise a flood of state-change narration during a slow turn would
    // queue up and be read back minutes late instead of catching up. The
    // gate keeps the worker stuck on the first message so the queue-depth
    // check isn't racing against how fast the backend happens to drain.
    let recorder = RecordingEventSink::new();
    let narrator = ScriptedNarrator::new((0..50).map(|_| Some("hola")).collect());
    let (backend, release) = BlockingSpeech::new();
    let speech = AsyncSpeech::new(Box::new(backend));

    let mut sink = SpeakingEventSink::new(recorder, narrator, speech.clone());
    for i in 0..50 {
        let task_id = TaskId::new();
        sink.emit(Event::LlmCompleted {
            llm_call_id: LlmCallId::new(&task_id, i),
            task_id,
            call_index: i,
            duration: std::time::Duration::from_millis(1),
        });
    }

    // Only 2 messages (the queue cap) ever reached the backend; releasing
    // twice and flushing proves it — if `say` (never-drop) had been used
    // instead, flush would hang here waiting on 48 releases never sent.
    release.send(()).unwrap();
    release.send(()).unwrap();
    speech.flush();
}
