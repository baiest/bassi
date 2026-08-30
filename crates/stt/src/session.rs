//! The listening state machine and the VAD smoother in front of it.
//!
//! Both are pure: they see booleans and probabilities, never audio,
//! hardware or wall-clock time. Everything is counted in **chunks** — at
//! 16 kHz a 512-sample chunk is exactly 32 ms — so every timer is an
//! integer count and the whole thing is deterministic under test.

/// Samples per chunk fed to the VAD. Silero at 16 kHz accepts only this
/// size, and it conveniently divides into exactly 32 ms.
pub const CHUNK_SAMPLES: usize = 512;

/// Milliseconds of audio in one chunk, at [`crate::WHISPER_SAMPLE_RATE`].
pub const CHUNK_MS: usize = 32;

const fn chunks_for_ms(ms: usize) -> u32 {
    (ms / CHUNK_MS) as u32
}

/// Tuning for [`Session`]. Every field is a chunk count derived from a
/// duration; the defaults are starting points meant to be tuned against a
/// real microphone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionConfig {
    /// Non-speech chunks required before an utterance is considered over.
    /// Long enough that a natural pause mid-sentence doesn't cut the user
    /// off.
    pub hangover: u32,
    /// How long the microphone stays open after Nala answers, waiting for
    /// a follow-up without the wake phrase.
    pub follow_up_window: u32,
    /// Hard cap on one utterance, so a stuck-open microphone can't buffer
    /// forever.
    pub max_utterance: u32,
    /// Speech chunks an utterance needs to be worth transcribing. Below
    /// this, Whisper is skipped entirely — on near-silence it hallucinates
    /// confident Spanish sentences.
    pub min_utterance: u32,
    /// Chunks ignored when listening resumes, to let the tail of Nala's
    /// own speech drain out of the microphone and the buffer.
    pub settle: u32,
    /// Chunks discarded at startup, while the input device ramps up and
    /// emits DC offset or zeros.
    pub cold_start: u32,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            hangover: chunks_for_ms(800),
            follow_up_window: chunks_for_ms(15_000),
            max_utterance: chunks_for_ms(20_000),
            min_utterance: chunks_for_ms(400),
            settle: chunks_for_ms(400),
            cold_start: chunks_for_ms(320),
        }
    }
}

/// Whether a turn requires the wake phrase or is a follow-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListenMode {
    /// The wake phrase must be heard before anything is captured.
    WakeWord,
    /// Capture starts on speech onset alone, for a limited window.
    FollowUp,
}

/// What the caller should do with the chunk it just reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Nothing to do; keep feeding chunks.
    Idle,
    /// Start accumulating audio from here (plus the configured pre-roll).
    StartCapture,
    /// Keep accumulating.
    Capture,
    /// Throw away what was captured and start over from here — the wake
    /// phrase fired again mid-utterance, which is how a self-correction
    /// ("oye Nala... ey Nala, ¿qué hora es?") sounds.
    RestartCapture,
    /// The utterance is complete; transcribe it.
    Complete,
    /// The utterance was too short to be worth transcribing; discard it.
    Discard,
    /// A follow-up window expired without any speech.
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Discarding the input device's startup garbage.
    ColdStart,
    /// Discarding the tail of Nala's own speech.
    Settling,
    /// Waiting for something to happen.
    Idle,
    /// Speech is happening but the wake phrase hasn't fired yet.
    Listening,
    /// Accumulating an utterance.
    Capturing,
}

/// The listening state machine.
///
/// Fed one chunk's worth of observations at a time via [`Session::observe`],
/// it decides when an utterance starts, ends, or should be thrown away.
pub struct Session {
    config: SessionConfig,
    mode: ListenMode,
    state: State,
    /// Chunks spent in the current state, for whichever timer applies.
    elapsed: u32,
    /// Non-speech chunks seen since the last speech chunk while capturing.
    silence: u32,
    /// Speech chunks accumulated in the current capture.
    speech_chunks: u32,
    /// Chunks captured so far, against `max_utterance`.
    captured: u32,
}

impl Session {
    pub fn new(config: SessionConfig, mode: ListenMode) -> Self {
        Self {
            config,
            mode,
            state: State::ColdStart,
            elapsed: 0,
            silence: 0,
            speech_chunks: 0,
            captured: 0,
        }
    }

    /// Starts a turn that skips the cold-start discard but still settles —
    /// used for every turn after the first, where the device is warm but
    /// Nala may just have finished speaking.
    pub fn resume(config: SessionConfig, mode: ListenMode) -> Self {
        let mut session = Self::new(config, mode);
        session.state = State::Settling;
        session
    }

    pub fn mode(&self) -> ListenMode {
        self.mode
    }

    /// Reports one chunk of observations and returns what to do with it.
    ///
    /// `speech` should already be smoothed by [`SpeechGate`] — it means
    /// "the user is talking", not "this 32 ms frame had energy".
    pub fn observe(&mut self, speech: bool, wake: bool) -> Action {
        match self.state {
            State::ColdStart => self.tick_discard(self.config.cold_start),
            State::Settling => self.tick_discard(self.config.settle),
            State::Idle => self.observe_idle(speech, wake),
            State::Listening => self.observe_listening(speech, wake),
            State::Capturing => self.observe_capturing(speech, wake),
        }
    }

    /// Burns through a fixed discard window, then opens for business.
    fn tick_discard(&mut self, window: u32) -> Action {
        self.elapsed += 1;
        if self.elapsed >= window {
            self.enter(State::Idle);
        }
        Action::Idle
    }

    fn observe_idle(&mut self, speech: bool, wake: bool) -> Action {
        if self.mode == ListenMode::FollowUp {
            // The window is a deadline for speech *onset* only. Once
            // capture starts it stops applying, so someone who begins
            // talking at 14.9 s isn't cut off mid-sentence.
            if speech {
                return self.start_capture();
            }

            self.elapsed += 1;
            if self.elapsed >= self.config.follow_up_window {
                return Action::Expired;
            }
            return Action::Idle;
        }

        // A wake phrase can land on the same chunk speech is first
        // detected, so check it before deciding to merely listen.
        if wake {
            return self.start_capture();
        }
        if speech {
            self.enter(State::Listening);
        }
        Action::Idle
    }

    fn observe_listening(&mut self, speech: bool, wake: bool) -> Action {
        if wake {
            return self.start_capture();
        }

        if speech {
            self.silence = 0;
        } else {
            self.silence += 1;
            // Someone spoke without the wake phrase — the cat, the TV, a
            // conversation. Once they stop, go back to waiting.
            if self.silence >= self.config.hangover {
                self.enter(State::Idle);
            }
        }

        Action::Idle
    }

    fn observe_capturing(&mut self, speech: bool, wake: bool) -> Action {
        if wake {
            self.captured = 1;
            self.speech_chunks = u32::from(speech);
            self.silence = 0;
            return Action::RestartCapture;
        }

        self.captured += 1;

        if speech {
            self.speech_chunks += 1;
            self.silence = 0;
        } else {
            self.silence += 1;
        }

        if self.captured >= self.config.max_utterance {
            // Hand over what we have rather than dropping it: losing the
            // tail of a long utterance beats losing all of it.
            self.enter(State::Idle);
            return Action::Complete;
        }

        if self.silence >= self.config.hangover {
            let complete = self.speech_chunks >= self.config.min_utterance;
            self.enter(State::Idle);
            return if complete {
                Action::Complete
            } else {
                Action::Discard
            };
        }

        Action::Capture
    }

    fn start_capture(&mut self) -> Action {
        self.enter(State::Capturing);
        self.captured = 1;
        self.speech_chunks = 0;
        self.silence = 0;
        Action::StartCapture
    }

    fn enter(&mut self, state: State) {
        self.state = state;
        self.elapsed = 0;
        self.silence = 0;
    }
}

/// Smooths raw VAD probabilities into a stable "the user is talking" flag.
///
/// Silero flips per-frame on plosives and breath, so feeding
/// `probability > 0.5` straight into [`Session`] would make an utterance
/// look like dozens of tiny ones. Two thresholds plus a short entry latch
/// fix that: speech has to be confident *and* sustained to latch, and only
/// drops out below a lower threshold.
pub struct SpeechGate {
    enter_threshold: f32,
    exit_threshold: f32,
    /// Consecutive above-threshold chunks required to latch on.
    enter_chunks: u32,
    consecutive: u32,
    active: bool,
}

impl Default for SpeechGate {
    fn default() -> Self {
        Self {
            enter_threshold: 0.55,
            exit_threshold: 0.35,
            enter_chunks: 2,
            consecutive: 0,
            active: false,
        }
    }
}

impl SpeechGate {
    pub fn new(enter_threshold: f32, exit_threshold: f32, enter_chunks: u32) -> Self {
        Self {
            enter_threshold,
            exit_threshold,
            enter_chunks,
            consecutive: 0,
            active: false,
        }
    }

    /// Feeds one chunk's probability and returns whether speech is active.
    pub fn update(&mut self, probability: f32) -> bool {
        if self.active {
            if probability < self.exit_threshold {
                self.active = false;
                self.consecutive = 0;
            }
            return self.active;
        }

        if probability > self.enter_threshold {
            self.consecutive += 1;
            if self.consecutive >= self.enter_chunks {
                self.active = true;
            }
        } else {
            // A probability between the two thresholds holds the current
            // state rather than resetting the run, so a single marginal
            // chunk mid-onset doesn't restart the latch.
            if probability < self.exit_threshold {
                self.consecutive = 0;
            }
        }

        self.active
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A config with small, easy-to-count windows.
    fn config() -> SessionConfig {
        SessionConfig {
            hangover: 3,
            follow_up_window: 5,
            max_utterance: 10,
            min_utterance: 2,
            settle: 2,
            cold_start: 2,
        }
    }

    /// Builds a session already past its cold-start window.
    fn ready(mode: ListenMode) -> Session {
        let mut session = Session::new(config(), mode);
        for _ in 0..config().cold_start {
            session.observe(false, false);
        }
        session
    }

    fn observe_n(session: &mut Session, count: u32, speech: bool, wake: bool) -> Vec<Action> {
        (0..count).map(|_| session.observe(speech, wake)).collect()
    }

    #[test]
    fn discards_the_first_chunks_while_the_device_warms_up() {
        let mut session = Session::new(config(), ListenMode::WakeWord);

        // Even a wake word during cold start is ignored.
        for _ in 0..config().cold_start {
            assert_eq!(session.observe(true, true), Action::Idle);
        }

        assert_eq!(session.observe(true, true), Action::StartCapture);
    }

    #[test]
    fn ignores_the_wake_word_while_settling_after_nala_speaks() {
        let mut session = Session::resume(config(), ListenMode::WakeWord);

        for _ in 0..config().settle {
            assert_eq!(session.observe(true, true), Action::Idle);
        }

        assert_eq!(session.observe(true, true), Action::StartCapture);
    }

    #[test]
    fn stays_idle_while_nothing_is_said() {
        let mut session = ready(ListenMode::WakeWord);

        assert!(
            observe_n(&mut session, 20, false, false)
                .iter()
                .all(|action| *action == Action::Idle)
        );
    }

    #[test]
    fn speech_without_the_wake_word_returns_to_idle_and_never_captures() {
        let mut session = ready(ListenMode::WakeWord);

        // Someone talking near the microphone — the cat, the TV.
        let spoken = observe_n(&mut session, 10, true, false);
        let silence = observe_n(&mut session, config().hangover, false, false);

        assert!(spoken.iter().all(|action| *action == Action::Idle));
        assert!(silence.iter().all(|action| *action == Action::Idle));
    }

    #[test]
    fn the_wake_word_starts_a_capture() {
        let mut session = ready(ListenMode::WakeWord);
        session.observe(true, false);

        assert_eq!(session.observe(true, true), Action::StartCapture);
        assert_eq!(session.observe(true, false), Action::Capture);
    }

    #[test]
    fn silence_for_the_hangover_completes_the_utterance() {
        let mut session = ready(ListenMode::WakeWord);
        session.observe(true, true);
        observe_n(&mut session, 4, true, false);

        let mut actions = observe_n(&mut session, config().hangover, false, false);

        assert_eq!(actions.pop(), Some(Action::Complete));
        assert!(actions.iter().all(|action| *action == Action::Capture));
    }

    #[test]
    fn a_pause_shorter_than_the_hangover_does_not_end_the_utterance() {
        let mut session = ready(ListenMode::WakeWord);
        session.observe(true, true);
        observe_n(&mut session, 3, true, false);

        // Pause, then keep talking.
        let pause = observe_n(&mut session, config().hangover - 1, false, false);
        assert!(pause.iter().all(|action| *action == Action::Capture));

        assert_eq!(session.observe(true, false), Action::Capture);

        // The silence counter reset, so a full hangover is needed again.
        let mut actions = observe_n(&mut session, config().hangover, false, false);
        assert_eq!(actions.pop(), Some(Action::Complete));
    }

    #[test]
    fn the_wake_word_during_a_capture_restarts_it() {
        let mut session = ready(ListenMode::WakeWord);
        session.observe(true, true);
        observe_n(&mut session, 3, true, false);

        assert_eq!(session.observe(true, true), Action::RestartCapture);

        // The restarted capture needs its own full utterance to complete.
        observe_n(&mut session, 2, true, false);
        let mut actions = observe_n(&mut session, config().hangover, false, false);
        assert_eq!(actions.pop(), Some(Action::Complete));
    }

    #[test]
    fn an_utterance_below_the_minimum_speech_length_is_discarded() {
        let mut session = ready(ListenMode::WakeWord);
        session.observe(true, true);

        // Only one speech chunk, below min_utterance of 2.
        session.observe(true, false);
        let mut actions = observe_n(&mut session, config().hangover, false, false);

        assert_eq!(actions.pop(), Some(Action::Discard));
    }

    #[test]
    fn the_max_utterance_cap_completes_rather_than_discarding() {
        let mut session = ready(ListenMode::WakeWord);
        session.observe(true, true);

        // Talk forever; the cap must fire before any hangover does.
        let mut actions = observe_n(&mut session, config().max_utterance - 1, true, false);

        assert_eq!(actions.pop(), Some(Action::Complete));
    }

    #[test]
    fn follow_up_captures_on_speech_onset_without_a_wake_word() {
        let mut session = ready(ListenMode::FollowUp);

        assert_eq!(session.observe(true, false), Action::StartCapture);
    }

    #[test]
    fn follow_up_expires_when_nothing_is_said() {
        let mut session = ready(ListenMode::FollowUp);

        let mut actions = observe_n(&mut session, config().follow_up_window, false, false);

        assert_eq!(actions.pop(), Some(Action::Expired));
        assert!(actions.iter().all(|action| *action == Action::Idle));
    }

    #[test]
    fn speech_on_the_last_chunk_of_the_follow_up_window_still_captures() {
        let mut session = ready(ListenMode::FollowUp);
        observe_n(&mut session, config().follow_up_window - 1, false, false);

        assert_eq!(session.observe(true, false), Action::StartCapture);
    }

    #[test]
    fn the_follow_up_deadline_stops_applying_once_capturing_begins() {
        let mut session = ready(ListenMode::FollowUp);
        session.observe(true, false);

        // Keep talking well past the follow-up window; it must not expire.
        let actions = observe_n(&mut session, config().follow_up_window + 2, true, false);

        assert!(actions.iter().all(|action| *action == Action::Capture));
    }

    #[test]
    fn a_single_confident_chunk_does_not_latch_speech() {
        let mut gate = SpeechGate::default();

        assert!(!gate.update(0.9));
    }

    #[test]
    fn two_consecutive_confident_chunks_latch_speech() {
        let mut gate = SpeechGate::default();

        gate.update(0.9);

        assert!(gate.update(0.9));
        assert!(gate.is_active());
    }

    #[test]
    fn a_probability_between_the_thresholds_holds_the_current_state() {
        let mut gate = SpeechGate::default();
        gate.update(0.9);
        gate.update(0.9);
        assert!(gate.is_active());

        // Between exit (0.35) and enter (0.55): stays on.
        assert!(gate.update(0.45));

        // Below exit: turns off.
        assert!(!gate.update(0.2));

        // And back on requires the full latch again.
        assert!(!gate.update(0.45));
    }
}
