use crate::ring::Ring;
use crate::session::{Action, CHUNK_SAMPLES, chunks_for_ms};
use crate::stream::AudioSource;
use crate::transcribe::{Transcribe, TranscribeError};
use crate::vad::VoiceDetector;
use crate::wake::{WakeDetector, strip_wake_prefix};

pub use crate::session::{ListenMode, Session, SessionConfig, SpeechGate};

/// A point in `listen()`'s progress worth surfacing to a user watching a
/// terminal — otherwise the whole pipeline is silent from the moment it
/// starts until it returns a finished transcript, which looks identical
/// whether it's idle, actively capturing, or stuck.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListenerStatus {
    /// Waiting for the wake phrase (or, in follow-up mode, for speech).
    Listening,
    /// The wake phrase (or follow-up speech onset) was just detected.
    Heard,
    /// The wake phrase fired again mid-capture (a self-correction, "oye
    /// Nala... ey Nala, ..."), discarding what had been captured so far.
    Restarted,
    /// Accumulating the utterance.
    Capturing,
    /// The utterance is complete; running it through Whisper.
    Transcribing,
    /// The utterance ended but was too short to be worth transcribing
    /// (below `min_utterance`) — discarded without calling Whisper.
    DiscardedTooShort,
    /// A capture transcribed to something empty or meaningless (a stray
    /// word, a hallucinated filler on near-silence) and was rejected.
    DiscardedNonsense,
}

#[derive(Debug, thiserror::Error)]
pub enum ListenError {
    #[error("audio source closed unexpectedly")]
    AudioSourceClosed,
    #[error("transcription failed: {0}")]
    Transcribe(#[from] TranscribeError),
}

/// Tuning for [`Listener`], layered on top of [`SessionConfig`].
#[derive(Debug, Clone, Copy)]
pub struct ListenerConfig {
    pub session: SessionConfig,
    /// Audio replayed into the wake detector when the VAD first latches
    /// onto speech, so its feature extraction isn't primed on a truncated
    /// onset. See BAS-25's plan for why this matters.
    pub wake_priming_chunks: usize,
    /// Audio prepended to a wake-triggered capture. Small: the command
    /// comes *after* the phrase, so this only needs to cover the
    /// detector's own reporting delay, not the phrase itself.
    pub wake_pre_roll_chunks: usize,
    /// Audio prepended to a follow-up-triggered capture. Larger: capture
    /// starts on speech onset, so this is what recovers the VAD's own
    /// latch latency.
    pub follow_up_pre_roll_chunks: usize,
}

impl Default for ListenerConfig {
    fn default() -> Self {
        Self {
            session: SessionConfig::default(),
            wake_priming_chunks: chunks_for_ms(320) as usize,
            wake_pre_roll_chunks: chunks_for_ms(200) as usize,
            follow_up_pre_roll_chunks: chunks_for_ms(500) as usize,
        }
    }
}

/// Ties the microphone, VAD, wake detector and transcriber together into
/// one blocking call: [`Listener::listen`].
pub struct Listener<A, V, W, T> {
    audio: A,
    vad: V,
    wake: W,
    transcriber: T,
    gate: SpeechGate,
    config: ListenerConfig,
    /// A short rolling window of raw chunks, independent of whatever
    /// buffering `audio` itself does — this is what pre-roll and wake
    /// priming replay from.
    recent: Ring,
    /// Whether this is the very first call: only then does the session
    /// run its cold-start discard instead of the shorter post-speech
    /// settle window.
    first_call: bool,
    on_status: Box<dyn FnMut(ListenerStatus)>,
}

impl<A, V, W, T> Listener<A, V, W, T>
where
    A: AudioSource,
    V: VoiceDetector,
    W: WakeDetector,
    T: Transcribe,
{
    pub fn new(audio: A, vad: V, wake: W, transcriber: T, config: ListenerConfig) -> Self {
        let recent_capacity = [
            config.wake_priming_chunks,
            config.wake_pre_roll_chunks,
            config.follow_up_pre_roll_chunks,
        ]
        .into_iter()
        .max()
        .unwrap_or(0)
            + 8; // margin so a peek never races the write it depends on

        Self {
            audio,
            vad,
            wake,
            transcriber,
            gate: SpeechGate::default(),
            config,
            recent: Ring::with_capacity(recent_capacity * CHUNK_SAMPLES),
            first_call: true,
            on_status: Box::new(|_| {}),
        }
    }

    /// Registers a callback fired on every [`ListenerStatus`] transition,
    /// so a caller (a terminal UI, a log line) can show what the pipeline
    /// is doing instead of it being silent until a transcript comes back.
    pub fn with_status<F: FnMut(ListenerStatus) + 'static>(mut self, on_status: F) -> Self {
        self.on_status = Box::new(on_status);
        self
    }

    /// Blocks until a complete utterance is captured and transcribed.
    ///
    /// `Ok(None)` only happens in [`ListenMode::FollowUp`]: either the
    /// window expired with no speech, or a follow-up capture's transcript
    /// didn't pass the sanity filter. Both are treated as "stop waiting
    /// for a follow-up" — one bad follow-up ends the chain rather than
    /// re-arming it, so background noise can't turn into an infinite loop
    /// of Nala answering the room. A [`ListenMode::WakeWord`] listen never
    /// returns `None`: a bad capture there is silently retried.
    pub fn listen(&mut self, mode: ListenMode) -> Result<Option<String>, ListenError> {
        let mut session = if self.first_call {
            self.first_call = false;
            Session::new(self.config.session, mode)
        } else {
            Session::resume(self.config.session, mode)
        };

        self.wake.reset();
        let mut capture: Vec<f32> = Vec::new();
        let mut chunk = [0.0_f32; CHUNK_SAMPLES];
        (self.on_status)(ListenerStatus::Listening);

        loop {
            if !self.audio.next_chunk(&mut chunk) {
                return Err(ListenError::AudioSourceClosed);
            }
            self.recent.write(&chunk);

            let probability = self.vad.probability(&chunk);
            let was_active = self.gate.is_active();
            let speech = self.gate.update(probability);

            if speech && !was_active {
                self.prime_wake_detector();
            }
            let wake = if speech {
                self.wake.detect(&chunk)
            } else {
                false
            };

            match session.observe(speech, wake) {
                Action::Idle => {}
                Action::StartCapture => {
                    (self.on_status)(ListenerStatus::Heard);
                    (self.on_status)(ListenerStatus::Capturing);
                    capture.clear();
                    self.prepend_pre_roll(&mut capture, mode);
                    capture.extend_from_slice(&chunk);
                }
                Action::RestartCapture => {
                    (self.on_status)(ListenerStatus::Restarted);
                    self.wake.reset();
                    capture.clear();
                    // A restart is always a wake-word self-correction —
                    // follow-up mode never re-triggers a wake word, so
                    // there is no follow-up variant of this pre-roll.
                    self.prepend_pre_roll(&mut capture, ListenMode::WakeWord);
                    capture.extend_from_slice(&chunk);
                }
                Action::Capture => {
                    capture.extend_from_slice(&chunk);
                }
                Action::Complete => {
                    self.wake.reset();
                    (self.on_status)(ListenerStatus::Transcribing);
                    let raw = self.transcriber.transcribe(&capture)?;
                    let text = strip_wake_prefix(&raw);

                    if is_sane(&text) {
                        return Ok(Some(text));
                    }
                    (self.on_status)(ListenerStatus::DiscardedNonsense);
                    // A well-formed but meaningless capture (a cough
                    // Whisper turns into a stray word, a hallucinated
                    // filler phrase on near-silence). In WakeWord mode
                    // that's not worth surfacing — keep listening
                    // silently. In FollowUp mode it's the "one bad
                    // follow-up ends the chain" rule.
                    if mode == ListenMode::FollowUp {
                        return Ok(None);
                    }
                    capture.clear();
                }
                Action::Discard => {
                    (self.on_status)(ListenerStatus::DiscardedTooShort);
                    self.wake.reset();
                    capture.clear();
                }
                Action::Expired => {
                    return Ok(None);
                }
            }
        }
    }

    fn prime_wake_detector(&mut self) {
        self.wake.reset();
        if self.config.wake_priming_chunks == 0 {
            return;
        }
        let mut priming = vec![0.0_f32; self.config.wake_priming_chunks * CHUNK_SAMPLES];
        if self.recent.peek_last(&mut priming) {
            self.wake.detect(&priming);
        }
    }

    fn prepend_pre_roll(&self, capture: &mut Vec<f32>, mode: ListenMode) {
        let chunks = match mode {
            ListenMode::WakeWord => self.config.wake_pre_roll_chunks,
            ListenMode::FollowUp => self.config.follow_up_pre_roll_chunks,
        };
        let mut pre_roll = vec![0.0_f32; chunks * CHUNK_SAMPLES];
        if self.recent.peek_last(&mut pre_roll) {
            capture.extend_from_slice(&pre_roll);
        }
    }
}

/// Rejects a transcript that's empty, whitespace, or has no alphanumeric
/// content — Whisper's confident hallucinations on near-silence tend to be
/// short filler or pure punctuation.
fn is_sane(text: &str) -> bool {
    !text.trim().is_empty() && text.chars().any(|c| c.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wake::WAKE_PHRASES;
    use std::collections::VecDeque;

    struct FakeAudioSource {
        chunks: VecDeque<f32>,
    }

    impl FakeAudioSource {
        /// One chunk per entry; `speech_at` marks which chunk indices carry
        /// a nonzero (arbitrary, unused by the fake VAD) marker — kept
        /// simple since the fake `VoiceDetector` scripts probabilities
        /// independently.
        fn silent(count: usize) -> Self {
            Self {
                chunks: std::iter::repeat_n(0.0, count * CHUNK_SAMPLES).collect(),
            }
        }
    }

    impl AudioSource for FakeAudioSource {
        fn next_chunk(&mut self, out: &mut [f32]) -> bool {
            if self.chunks.len() < out.len() {
                return false;
            }
            for slot in out.iter_mut() {
                *slot = self.chunks.pop_front().unwrap();
            }
            true
        }
    }

    /// Reports a scripted probability per chunk, then holds the last one.
    struct FakeVad {
        probabilities: VecDeque<f32>,
    }

    impl FakeVad {
        fn scripted(probabilities: Vec<f32>) -> Self {
            Self {
                probabilities: probabilities.into(),
            }
        }
    }

    impl VoiceDetector for FakeVad {
        fn probability(&mut self, _chunk: &[f32]) -> f32 {
            self.probabilities.pop_front().unwrap_or(0.0)
        }
    }

    /// Fires exactly once, on the Nth call to `detect`.
    struct FakeWake {
        fire_on_call: usize,
        calls: usize,
    }

    impl FakeWake {
        fn firing_on_call(fire_on_call: usize) -> Self {
            Self {
                fire_on_call,
                calls: 0,
            }
        }

        fn never() -> Self {
            Self {
                fire_on_call: usize::MAX,
                calls: 0,
            }
        }
    }

    impl WakeDetector for FakeWake {
        fn detect(&mut self, _chunk: &[f32]) -> bool {
            self.calls += 1;
            self.calls == self.fire_on_call
        }

        fn reset(&mut self) {}
    }

    struct FakeTranscribe {
        text: String,
    }

    impl FakeTranscribe {
        fn returning(text: &str) -> Self {
            Self {
                text: text.to_string(),
            }
        }
    }

    impl Transcribe for FakeTranscribe {
        fn transcribe(&self, _samples: &[f32]) -> Result<String, TranscribeError> {
            Ok(self.text.clone())
        }
    }

    fn config() -> ListenerConfig {
        ListenerConfig {
            session: SessionConfig {
                hangover: 2,
                follow_up_window: 5,
                max_utterance: 20,
                min_utterance: 1,
                settle: 0,
                cold_start: 0,
            },
            wake_priming_chunks: 0,
            wake_pre_roll_chunks: 1,
            follow_up_pre_roll_chunks: 1,
        }
    }

    /// The chunk sequence shared by most tests below: chunk 1 is always
    /// consumed by the (zero-length, but still one chunk) cold-start
    /// window, regardless of its content; two consecutive high
    /// probabilities are needed to latch the `SpeechGate` (chunks 2-3); the
    /// wake word — when it fires — does so on the transition chunk (3),
    /// which the FSM does *not* count towards `min_utterance` since
    /// `start_capture` resets the speech counter; chunk 4 supplies the one
    /// counted speech chunk; chunks 5-6 are the hangover that completes
    /// the utterance.
    fn wake_triggered_sequence() -> (Vec<f32>, usize) {
        (vec![0.0, 0.9, 0.9, 0.9, 0.0, 0.0], 6)
    }

    #[test]
    fn a_wake_word_then_speech_yields_the_transcribed_text() {
        let (probabilities, chunks) = wake_triggered_sequence();
        let vad = FakeVad::scripted(probabilities);
        let audio = FakeAudioSource::silent(chunks);
        let wake = FakeWake::firing_on_call(1);
        let transcriber = FakeTranscribe::returning("oye nala que hora es");

        let mut listener = Listener::new(audio, vad, wake, transcriber, config());

        let result = listener.listen(ListenMode::WakeWord).unwrap();

        assert_eq!(result, Some("que hora es".to_string()));
    }

    #[test]
    fn an_empty_transcript_is_rejected_and_listening_resumes() {
        // The utterance completes with a blank transcript, which is
        // rejected and silently retried in WakeWord mode; the audio
        // source is exhausted right after, so the retry surfaces as a
        // clean AudioSourceClosed rather than hanging.
        let (probabilities, chunks) = wake_triggered_sequence();
        let vad = FakeVad::scripted(probabilities);
        let audio = FakeAudioSource::silent(chunks);
        let wake = FakeWake::firing_on_call(1);
        let transcriber = FakeTranscribe::returning("   ");

        let mut listener = Listener::new(audio, vad, wake, transcriber, config());

        let result = listener.listen(ListenMode::WakeWord);

        assert!(matches!(result, Err(ListenError::AudioSourceClosed)));
    }

    #[test]
    fn a_leading_wake_phrase_is_stripped_from_the_transcript() {
        for phrase in WAKE_PHRASES {
            let (probabilities, chunks) = wake_triggered_sequence();
            let vad = FakeVad::scripted(probabilities);
            let audio = FakeAudioSource::silent(chunks);
            let wake = FakeWake::firing_on_call(1);
            let transcriber = FakeTranscribe::returning(&format!("{phrase} apagá la luz"));

            let mut listener = Listener::new(audio, vad, wake, transcriber, config());
            let result = listener.listen(ListenMode::WakeWord).unwrap();

            assert_eq!(result, Some("apagá la luz".to_string()));
        }
    }

    #[test]
    fn a_follow_up_whose_transcript_fails_the_sanity_filter_does_not_rearm() {
        // Same chunk shape as the wake-word sequence, minus needing the
        // wake word itself: speech onset alone starts the capture.
        let (probabilities, chunks) = wake_triggered_sequence();
        let vad = FakeVad::scripted(probabilities);
        let audio = FakeAudioSource::silent(chunks);
        let wake = FakeWake::never(); // follow-up never needs the wake word
        let transcriber = FakeTranscribe::returning(".");

        let mut listener = Listener::new(audio, vad, wake, transcriber, config());

        let result = listener.listen(ListenMode::FollowUp).unwrap();

        assert_eq!(
            result, None,
            "a nonsense follow-up transcript must end the chain, not be surfaced"
        );
    }

    #[test]
    fn with_status_reports_the_expected_transitions_in_order() {
        let (probabilities, chunks) = wake_triggered_sequence();
        let vad = FakeVad::scripted(probabilities);
        let audio = FakeAudioSource::silent(chunks);
        let wake = FakeWake::firing_on_call(1);
        let transcriber = FakeTranscribe::returning("oye nala que hora es");

        let statuses = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let recorded = std::rc::Rc::clone(&statuses);

        let mut listener = Listener::new(audio, vad, wake, transcriber, config())
            .with_status(move |status| recorded.borrow_mut().push(status));

        listener.listen(ListenMode::WakeWord).unwrap();

        assert_eq!(
            *statuses.borrow(),
            vec![
                ListenerStatus::Listening,
                ListenerStatus::Heard,
                ListenerStatus::Capturing,
                ListenerStatus::Transcribing,
            ]
        );
    }

    #[test]
    fn follow_up_expiry_with_no_speech_returns_none() {
        // Chunk 1 is consumed by cold start regardless of content; the
        // remaining `follow_up_window` (5) silent chunks exhaust the
        // window.
        let vad = FakeVad::scripted(vec![0.0; 6]);
        let audio = FakeAudioSource::silent(6);
        let wake = FakeWake::never();
        let transcriber = FakeTranscribe::returning("unused");

        let mut listener = Listener::new(audio, vad, wake, transcriber, config());

        let result = listener.listen(ListenMode::FollowUp).unwrap();

        assert_eq!(result, None);
    }
}
