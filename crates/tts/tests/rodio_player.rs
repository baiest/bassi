use std::sync::mpsc;

use tts::{PcmStream, PlayPcmStream, RodioPlayer, SpeechError};

fn stream_of(chunks: Vec<Result<Vec<i16>, SpeechError>>) -> PcmStream {
    let (sender, receiver) = mpsc::channel();
    for chunk in chunks {
        let _ = sender.send(chunk);
    }
    // Dropping `sender` here closes the channel once the buffered chunks
    // are drained, which is what signals end-of-stream to the player.
    PcmStream {
        sample_rate: 24_000,
        channels: 1,
        chunks: receiver,
    }
}

#[test]
fn player_plays_an_empty_stream_without_error() {
    let Ok(player) = RodioPlayer::new() else {
        // No audio device available on this machine (e.g. CI runner) -
        // nothing to assert about playback.
        return;
    };

    let result = player.play_stream(stream_of(vec![]));

    assert!(result.is_ok());
}

#[test]
fn player_propagates_an_error_chunk() {
    let Ok(player) = RodioPlayer::new() else {
        return;
    };

    let result = player.play_stream(stream_of(vec![
        Ok(vec![0, 0]),
        Err(SpeechError::Synthesis("stream broke mid-way".into())),
    ]));

    assert!(matches!(result, Err(SpeechError::Synthesis(_))));
}
