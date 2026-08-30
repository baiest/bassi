#[path = "common/fake_tts.rs"]
mod fake_tts;

use fake_tts::{FakeSynth, SpyPlayer};
use tts::{PcmStream, PlayPcmStream, Speech, SpeechError, StreamSynthesizeSpeech, StreamingSpeech};

#[test]
fn speech_sends_text_to_synthesizer_and_plays_returned_samples() {
    use std::sync::Arc;

    struct SharedSynth(Arc<FakeSynth>);
    impl StreamSynthesizeSpeech for SharedSynth {
        fn synthesize_stream(&self, text: &str) -> Result<PcmStream, SpeechError> {
            self.0.synthesize_stream(text)
        }
    }

    struct SharedPlayer(Arc<SpyPlayer>);
    impl PlayPcmStream for SharedPlayer {
        fn play_stream(&self, stream: PcmStream) -> Result<(), SpeechError> {
            self.0.play_stream(stream)
        }
    }

    let synth = Arc::new(FakeSynth::returning(vec![9, 9, 9]));
    let player = Arc::new(SpyPlayer::new());

    let speech = StreamingSpeech::new(
        Box::new(SharedSynth(synth.clone())),
        Box::new(SharedPlayer(player.clone())),
    );

    speech.say("texto de prueba").unwrap();

    assert_eq!(synth.received(), vec!["texto de prueba".to_string()]);
    assert_eq!(player.played(), vec![9, 9, 9]);
}

#[test]
fn synthesis_failure_is_not_played() {
    use std::sync::Arc;

    struct SharedPlayer(Arc<SpyPlayer>);
    impl PlayPcmStream for SharedPlayer {
        fn play_stream(&self, stream: PcmStream) -> Result<(), SpeechError> {
            self.0.play_stream(stream)
        }
    }

    let player = Arc::new(SpyPlayer::new());
    let synth = FakeSynth::failing(SpeechError::Unavailable("down".into()));

    let speech = StreamingSpeech::new(Box::new(synth), Box::new(SharedPlayer(player.clone())));

    let result = speech.say("no deberia sonar");

    assert!(matches!(result, Err(SpeechError::Unavailable(_))));
    assert!(player.played().is_empty());
}

#[test]
fn playback_failure_propagates() {
    let synth = FakeSynth::returning(vec![1]);
    let player = SpyPlayer::failing();

    let speech = StreamingSpeech::new(Box::new(synth), Box::new(player));

    let result = speech.say("texto");

    assert!(matches!(result, Err(SpeechError::Playback(_))));
}
