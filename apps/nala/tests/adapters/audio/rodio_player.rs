use nala::adapters::audio::rodio_player::RodioPlayer;
use nala::adapters::speech::chatterbox::PlayAudio;
use nala::ports::speech::SpeechError;

#[test]
fn player_rejects_corrupt_wav() {
    // No real output device is opened for corrupt input: the decoder
    // rejects the bytes before anything would reach the audio device, so
    // this is safe to run in CI/headless environments.
    let Ok(player) = RodioPlayer::new() else {
        // No audio device available on this machine (e.g. CI runner) -
        // nothing to assert about decoding.
        return;
    };

    let result = player.play(b"this is not a wav file");

    assert!(matches!(result, Err(SpeechError::Playback(_))));
}
