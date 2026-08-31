use crate::audio::RodioPlayer;
use crate::chatterbox::config::ChatterboxConfig;
use crate::chatterbox::{ChatterboxSupervisor, HttpChatterbox};
use crate::pcm::StreamSynthesizeSpeech;
use crate::piper::PiperSpeech;
use crate::piper::config::PiperConfig;
use crate::speech::{Speech, SpeechError};
use crate::streaming_speech::StreamingSpeech;
use crate::windows_sapi::WindowsSapiSpeech;

/// No-op `Speech` backend, used when `NALA_TTS=off` (or on a non-Windows
/// build). Keeps callers that unconditionally wire up an `AsyncSpeech`
/// working without branching their own type on an env var only known at
/// runtime — it just produces no audio.
pub struct NullSpeech;

impl Speech for NullSpeech {
    fn say(&self, _text: &str) -> Result<(), SpeechError> {
        Ok(())
    }
}

/// Resolves the TTS backend from `NALA_TTS` (`piper` | `chatterbox` |
/// `sapi` | `off`, default `piper`). Also returns the `ChatterboxSupervisor`
/// when one was started, so the caller can keep it alive - dropping it
/// kills the server process it spawned. Piper has no supervisor: it's a
/// per-utterance child process, nothing to keep alive between turns.
///
/// Piper is the default because it answers about as fast as SAPI while
/// sounding noticeably more natural - Chatterbox's cloned voice is more
/// natural still, but synthesis is CPU-bound and much slower, so it stays
/// opt-in via `NALA_TTS=chatterbox`.
///
/// Whichever backend is selected is never allowed to leave the caller mute:
/// any failure building it (missing binary/model/reference, server
/// unreachable, no audio device, ...) logs a warning and falls back to
/// Windows SAPI instead of propagating.
pub fn speech_backend() -> (Box<dyn Speech + Send>, Option<ChatterboxSupervisor>) {
    match std::env::var("NALA_TTS").as_deref() {
        Ok("off") => (Box::new(NullSpeech), None),
        Ok("sapi") => (Box::new(WindowsSapiSpeech::new()), None),
        Ok("chatterbox") => match build_chatterbox() {
            Ok((speech, supervisor)) => (speech, Some(supervisor)),
            Err(error) => {
                eprintln!(
                    "Warning: Chatterbox TTS unavailable ({error}); falling back to Windows SAPI."
                );
                (Box::new(WindowsSapiSpeech::new()), None)
            }
        },
        _ => match build_piper() {
            Ok(speech) => (speech, None),
            Err(error) => {
                eprintln!(
                    "Warning: Piper TTS unavailable ({error}); falling back to Windows SAPI."
                );
                (Box::new(WindowsSapiSpeech::new()), None)
            }
        },
    }
}

/// Resolves a raw `StreamSynthesizeSpeech` from `NALA_TTS` (`piper` |
/// `chatterbox`, default `piper`) — the PCM producer without the
/// player/`AsyncSpeech` wrapping `speech_backend` adds, for a caller that
/// wants the synthesized audio itself (e.g. to forward it over a socket)
/// rather than to play it on this machine. `sapi`/`off` aren't PCM-stream
/// backends, so they're not offered here; unlike `speech_backend`, failures
/// are returned rather than silently falling back, since there's no local
/// speaker to fall back to being audible on.
pub fn stream_synthesizer() -> Result<
    (
        Box<dyn StreamSynthesizeSpeech + Send>,
        Option<ChatterboxSupervisor>,
    ),
    SpeechError,
> {
    match std::env::var("NALA_TTS").as_deref() {
        Ok("chatterbox") => {
            let config = ChatterboxConfig::from_env()?;
            let supervisor = ChatterboxSupervisor::ensure_running(&config)?;
            let synth = HttpChatterbox::new(
                &config.base_url,
                &config.voice,
                config.exaggeration,
                config.cfg_weight,
                config.temperature,
                &config.streaming_strategy,
                config.streaming_chunk_size,
                config.timeout,
                config.read_timeout,
            );
            Ok((Box::new(synth), Some(supervisor)))
        }
        _ => {
            let config = PiperConfig::from_env()?;
            Ok((Box::new(PiperSpeech::new(config)), None))
        }
    }
}

fn build_piper() -> Result<Box<dyn Speech + Send>, SpeechError> {
    let config = PiperConfig::from_env()?;
    let synth = PiperSpeech::new(config);
    let player = RodioPlayer::new()?;

    Ok(Box::new(StreamingSpeech::new(
        Box::new(synth),
        Box::new(player),
    )))
}

fn build_chatterbox() -> Result<(Box<dyn Speech + Send>, ChatterboxSupervisor), SpeechError> {
    let config = ChatterboxConfig::from_env()?;
    let supervisor = ChatterboxSupervisor::ensure_running(&config)?;

    let synth = HttpChatterbox::new(
        &config.base_url,
        &config.voice,
        config.exaggeration,
        config.cfg_weight,
        config.temperature,
        &config.streaming_strategy,
        config.streaming_chunk_size,
        config.timeout,
        config.read_timeout,
    );
    let player = RodioPlayer::new()?;

    Ok((
        Box::new(StreamingSpeech::new(Box::new(synth), Box::new(player))),
        supervisor,
    ))
}
