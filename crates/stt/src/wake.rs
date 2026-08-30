use crate::transcribe::Transcribe;

/// The wake phrases Nala answers to.
///
/// All three are **prefixed**. A bare "Nala" is deliberately absent and
/// must never trigger: the user's cat is also called Nala, and no
/// sound-only detector can tell "Nala, ¿qué hora es?" from "Nala, bajate
/// de la mesa" — the acoustics are identical. Requiring a prefix is what
/// disambiguates, and checking it in text (after transcription) rather
/// than audio is what makes the check reliable: it is language
/// understanding, not signal matching.
pub const WAKE_PHRASES: &[&str] = &["oye nala", "ey nala", "ve nala"];

/// Reports whether accumulated audio completed a wake phrase.
///
/// A trait so the state machine can be driven by a fake that fires on a
/// chosen chunk, with no model files or audio hardware, and so the
/// implementation can be swapped for a dedicated wake-word engine later
/// without touching anything downstream.
pub trait WakeDetector {
    /// Feeds one chunk and reports whether a wake phrase just completed.
    /// `chunk` is [`crate::CHUNK_SAMPLES`] long at
    /// [`crate::WHISPER_SAMPLE_RATE`].
    fn detect(&mut self, chunk: &[f32]) -> bool;

    /// Clears accumulated audio. Called when the pipeline returns to idle,
    /// so a half-heard phrase can't complete minutes later against
    /// unrelated audio.
    fn reset(&mut self);
}

/// How much audio to accumulate before attempting a transcription. Short
/// enough to catch the wake phrase without much delay, long enough to
/// contain it — "oye Nala" is roughly 700 ms spoken at a normal pace.
const CHECK_INTERVAL_CHUNKS: usize = 25; // 800 ms at 32 ms/chunk

/// Caps how much audio is kept without a detection, so a long ramble
/// that's never prefixed doesn't grow the buffer (or the transcription
/// cost) without bound. Sized for the longest wake phrase plus margin.
const MAX_BUFFER_CHUNKS: usize = 63; // ~2 s

/// Detects the wake phrase by transcribing accumulated audio and checking
/// whether it starts with one of [`WAKE_PHRASES`].
///
/// This is the second stage of the cascade: only fed while the VAD says
/// someone is talking, so it costs nothing in a quiet room. It trades a
/// dedicated wake-word engine's low latency and CPU cost for something no
/// acoustic matcher can do — telling "Nala, ¿qué hora es?" from "Nala,
/// bajate de la mesa" by what was actually said. See BAS-25 for why: every
/// wake-word crate evaluated either needed a trained model with no
/// Spanish/custom-phrase option, or has an unmaintained Rust SDK.
pub struct WhisperWake<T: Transcribe> {
    transcriber: T,
    buffer: Vec<f32>,
    since_check: usize,
    /// Fired with the raw transcript on every check, whether or not it
    /// matched a wake phrase — otherwise a wake-triggered check that
    /// doesn't match is invisible: nothing else in the pipeline reports
    /// what was actually heard.
    on_check: Box<dyn FnMut(&str)>,
}

impl<T: Transcribe> WhisperWake<T> {
    pub fn new(transcriber: T) -> Self {
        Self {
            transcriber,
            buffer: Vec::with_capacity(MAX_BUFFER_CHUNKS * crate::CHUNK_SAMPLES),
            since_check: 0,
            on_check: Box::new(|_| {}),
        }
    }

    /// Registers a callback fired with the transcript of every wake-word
    /// check, matched or not.
    pub fn with_check_callback<F: FnMut(&str) + 'static>(mut self, on_check: F) -> Self {
        self.on_check = Box::new(on_check);
        self
    }

    fn is_wake_phrase(text: &str) -> bool {
        matching_wake_phrase_len(&words_with_end_offsets(text)).is_some()
    }
}

impl<T: Transcribe> WakeDetector for WhisperWake<T> {
    fn detect(&mut self, chunk: &[f32]) -> bool {
        self.buffer.extend_from_slice(chunk);
        self.since_check += 1;

        if self.since_check < CHECK_INTERVAL_CHUNKS {
            return false;
        }
        self.since_check = 0;

        let detected = match self.transcriber.transcribe(&self.buffer) {
            Ok(text) => {
                (self.on_check)(&text);
                Self::is_wake_phrase(&text)
            }
            // A transcription failure isn't a wake event, but it also
            // isn't grounds to stop listening — treat it as silence.
            Err(_) => false,
        };

        if detected {
            self.buffer.clear();
        } else if self.buffer.len() > MAX_BUFFER_CHUNKS * crate::CHUNK_SAMPLES {
            // Nothing detected and the buffer is past its cap: drop the
            // oldest audio rather than growing forever or re-checking the
            // same stale prefix on every future chunk.
            let excess = self.buffer.len() - MAX_BUFFER_CHUNKS * crate::CHUNK_SAMPLES;
            self.buffer.drain(..excess);
        }

        detected
    }

    fn reset(&mut self) {
        self.buffer.clear();
        self.since_check = 0;
    }
}

/// Removes a leading wake phrase from a transcript.
///
/// Belt and braces alongside the capture offset: the audio handed to
/// Whisper for the actual command starts shortly before the detection
/// point, so "oye Nala" can still bleed into it, and Nala shouldn't
/// receive her own wake phrase as part of the request.
pub fn strip_wake_prefix(transcript: &str) -> String {
    let words = words_with_end_offsets(transcript);

    match matching_wake_phrase_len(&words) {
        Some(prefix_len) => {
            let cut = words[prefix_len - 1].1;
            transcript[cut..]
                .trim_start_matches(|c: char| !c.is_alphanumeric())
                .trim()
                .to_string()
        }
        None => transcript.trim().to_string(),
    }
}

/// Extracts the letters/digits-only, lowercased words in `text`, each
/// paired with the byte offset immediately after it in the *original*
/// string — punctuation-tolerant matching needs the normalized form, but
/// [`strip_wake_prefix`] needs to cut the real transcript at the right
/// point, preserving whatever casing/punctuation follows the phrase.
fn words_with_end_offsets(text: &str) -> Vec<(String, usize)> {
    let mut words = Vec::new();
    let mut start: Option<usize> = None;

    for (i, c) in text.char_indices() {
        if c.is_alphanumeric() {
            start.get_or_insert(i);
        } else if let Some(s) = start.take() {
            words.push((text[s..i].to_lowercase(), i));
        }
    }
    if let Some(s) = start {
        words.push((text[s..].to_lowercase(), text.len()));
    }

    words
}

/// If `words` begins with one of [`WAKE_PHRASES`], returns how many words
/// that phrase occupies.
///
/// Matching whole words rather than a literal substring is what makes this
/// tolerant of whatever punctuation Whisper inserts — "Oye, Nala." and
/// "oye nala" both match, where a plain `starts_with` would not.
fn matching_wake_phrase_len(words: &[(String, usize)]) -> Option<usize> {
    WAKE_PHRASES.iter().find_map(|phrase| {
        let phrase_words: Vec<&str> = phrase.split(' ').collect();
        let matches = words.len() >= phrase_words.len()
            && phrase_words
                .iter()
                .enumerate()
                .all(|(i, word)| words[i].0 == *word);

        matches.then_some(phrase_words.len())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcribe::TranscribeError;
    use std::cell::RefCell;

    /// A scripted `Transcribe` that returns canned text and records what
    /// it was asked to transcribe.
    struct FakeTranscriber {
        responses: RefCell<std::collections::VecDeque<Result<String, TranscribeError>>>,
        calls: RefCell<Vec<usize>>,
    }

    impl FakeTranscriber {
        fn returning(responses: Vec<&str>) -> Self {
            Self {
                responses: RefCell::new(
                    responses
                        .into_iter()
                        .map(|text| Ok(text.to_string()))
                        .collect(),
                ),
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl Transcribe for FakeTranscriber {
        fn transcribe(&self, samples: &[f32]) -> Result<String, TranscribeError> {
            self.calls.borrow_mut().push(samples.len());
            self.responses
                .borrow_mut()
                .pop_front()
                .unwrap_or_else(|| Ok(String::new()))
        }
    }

    fn feed_chunks(detector: &mut WhisperWake<FakeTranscriber>, count: usize) -> bool {
        let chunk = vec![0.0_f32; crate::CHUNK_SAMPLES];
        let mut detected = false;
        for _ in 0..count {
            if detector.detect(&chunk) {
                detected = true;
            }
        }
        detected
    }

    #[test]
    fn with_check_callback_reports_every_transcript_even_without_a_match() {
        let transcriber = FakeTranscriber::returning(vec!["ruido de fondo"]);
        let heard = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let recorded = std::rc::Rc::clone(&heard);

        let mut detector = WhisperWake::new(transcriber)
            .with_check_callback(move |text| recorded.borrow_mut().push(text.to_string()));

        feed_chunks(&mut detector, CHECK_INTERVAL_CHUNKS);

        assert_eq!(*heard.borrow(), vec!["ruido de fondo".to_string()]);
    }

    #[test]
    fn does_not_check_before_the_interval_elapses() {
        let transcriber = FakeTranscriber::returning(vec!["oye nala que hora es"]);
        let mut detector = WhisperWake::new(transcriber);

        assert!(!feed_chunks(&mut detector, CHECK_INTERVAL_CHUNKS - 1));
    }

    #[test]
    fn detects_a_wake_phrase_at_the_start_of_the_transcript() {
        let transcriber = FakeTranscriber::returning(vec!["oye nala que hora es"]);
        let mut detector = WhisperWake::new(transcriber);

        assert!(feed_chunks(&mut detector, CHECK_INTERVAL_CHUNKS));
    }

    #[test]
    fn does_not_detect_a_bare_name_with_no_prefix() {
        // The cat case: the transcript exists, but doesn't start with any
        // of the wake phrases.
        let transcriber = FakeTranscriber::returning(vec!["nala bajate de la mesa"]);
        let mut detector = WhisperWake::new(transcriber);

        assert!(!feed_chunks(&mut detector, CHECK_INTERVAL_CHUNKS));
    }

    #[test]
    fn ignores_a_transcription_failure_instead_of_erroring() {
        let transcriber = FakeTranscriber {
            responses: RefCell::new(
                vec![Err(TranscribeError::Transcription("boom".to_string()))].into(),
            ),
            calls: RefCell::new(Vec::new()),
        };
        let mut detector = WhisperWake::new(transcriber);

        assert!(!feed_chunks(&mut detector, CHECK_INTERVAL_CHUNKS));
    }

    #[test]
    fn reset_clears_the_buffer_so_a_stale_phrase_cannot_complete_later() {
        let transcriber = FakeTranscriber::returning(vec!["nada", "oye nala hola"]);
        let mut detector = WhisperWake::new(transcriber);

        feed_chunks(&mut detector, CHECK_INTERVAL_CHUNKS);
        detector.reset();

        // Without the reset, the leftover buffer plus a few more chunks
        // would reach the next check with stale audio already inside.
        assert!(feed_chunks(&mut detector, CHECK_INTERVAL_CHUNKS));
    }

    #[test]
    fn strips_a_leading_wake_phrase() {
        assert_eq!(strip_wake_prefix("oye Nala, ¿qué hora es?"), "qué hora es?");
    }

    #[test]
    fn tolerates_punctuation_whisper_inserts_between_words() {
        // The exact transcript observed during manual testing: Whisper
        // punctuates "oye Nala" as two words with a comma and a period,
        // which a literal substring match would never recognize.
        assert!(WhisperWake::<FakeTranscriber>::is_wake_phrase("Oye, Nala."));
        assert_eq!(
            strip_wake_prefix("Oye, Nala. ¿Qué hora es?"),
            "Qué hora es?"
        );
    }

    #[test]
    fn strips_every_supported_variant() {
        assert_eq!(strip_wake_prefix("ey Nala apagá la luz"), "apagá la luz");
        assert_eq!(strip_wake_prefix("ve Nala apagá la luz"), "apagá la luz");
    }

    #[test]
    fn ignores_leading_punctuation_before_the_phrase() {
        assert_eq!(
            strip_wake_prefix("  ¡Oye Nala! ¿qué hora es?"),
            "qué hora es?"
        );
    }

    #[test]
    fn leaves_a_transcript_without_a_wake_phrase_alone() {
        assert_eq!(strip_wake_prefix("  ¿qué hora es?  "), "¿qué hora es?");
    }

    #[test]
    fn does_not_strip_a_bare_name() {
        assert_eq!(
            strip_wake_prefix("Nala, bajate de la mesa"),
            "Nala, bajate de la mesa"
        );
    }
}
