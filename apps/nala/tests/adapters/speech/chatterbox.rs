use nala::adapters::speech::chatterbox::speech::ChatterboxSpeech;
use nala::ports::speech::{Speech, SpeechError};

use crate::fake_tts::{FakeSynth, SpyPlayer};

#[test]
fn speech_sends_text_to_synthesizer_and_plays_returned_bytes() {
    use std::sync::Arc;

    struct SharedSynth(Arc<FakeSynth>);
    impl nala::adapters::speech::chatterbox::SynthesizeSpeech for SharedSynth {
        fn synthesize(&self, text: &str) -> Result<Vec<u8>, SpeechError> {
            self.0.synthesize(text)
        }
    }

    struct SharedPlayer(Arc<SpyPlayer>);
    impl nala::adapters::speech::chatterbox::PlayAudio for SharedPlayer {
        fn play(&self, audio: &[u8]) -> Result<(), SpeechError> {
            self.0.play(audio)
        }
    }

    let synth = Arc::new(FakeSynth::returning(vec![9, 9, 9]));
    let player = Arc::new(SpyPlayer::new());

    let speech = ChatterboxSpeech::new(
        Box::new(SharedSynth(synth.clone())),
        Box::new(SharedPlayer(player.clone())),
    );

    speech.say("texto de prueba").unwrap();

    assert_eq!(synth.received(), vec!["texto de prueba".to_string()]);
    assert_eq!(player.played(), vec![vec![9, 9, 9]]);
}

#[test]
fn synthesis_failure_is_not_played() {
    use std::sync::Arc;

    struct SharedPlayer(Arc<SpyPlayer>);
    impl nala::adapters::speech::chatterbox::PlayAudio for SharedPlayer {
        fn play(&self, audio: &[u8]) -> Result<(), SpeechError> {
            self.0.play(audio)
        }
    }

    let player = Arc::new(SpyPlayer::new());
    let synth = FakeSynth::failing(SpeechError::Unavailable("down".into()));

    let speech = ChatterboxSpeech::new(Box::new(synth), Box::new(SharedPlayer(player.clone())));

    let result = speech.say("no deberia sonar");

    assert!(matches!(result, Err(SpeechError::Unavailable(_))));
    assert!(player.played().is_empty());
}

#[test]
fn playback_failure_propagates() {
    let synth = FakeSynth::returning(vec![1]);
    let player = SpyPlayer::failing();

    let speech = ChatterboxSpeech::new(Box::new(synth), Box::new(player));

    let result = speech.say("texto");

    assert!(matches!(result, Err(SpeechError::Playback(_))));
}
