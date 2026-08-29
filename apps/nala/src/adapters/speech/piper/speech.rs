use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;

use crate::adapters::speech::pcm::{PcmStream, StreamSynthesizeSpeech, stream_pcm_from};
use crate::ports::speech::SpeechError;

use super::config::PiperConfig;

/// Synthesizes speech by spawning Piper's CLI per utterance and reading raw
/// PCM from its stdout. Piper starts up and produces audio fast enough
/// (small non-autoregressive model, no server process to keep alive) that a
/// fresh process per `say` call is simpler than pooling one, unlike
/// Chatterbox's long-lived HTTP server.
pub struct PiperSpeech {
    config: PiperConfig,
}

impl PiperSpeech {
    pub fn new(config: PiperConfig) -> Self {
        Self { config }
    }
}

/// Piper treats each stdin *line* as a separate utterance, so a
/// multi-line answer (blank lines, wrapped text) would otherwise come out
/// chopped into several utterances with gaps between them. Collapsing
/// whitespace runs - including newlines - into single spaces keeps it one
/// utterance without changing the words spoken. Pulled out as a pure
/// function so this is testable without spawning Piper.
pub fn normalize_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Builds the `piper` CLI arguments for `config`. A pure function so the
/// exact flags Piper receives can be asserted without spawning a process.
pub fn build_args(config: &PiperConfig) -> Vec<String> {
    let mut args = vec![
        "--model".to_string(),
        config.model_path.display().to_string(),
        "--output_raw".to_string(),
        "--length_scale".to_string(),
        config.length_scale.to_string(),
        "--noise_scale".to_string(),
        config.noise_scale.to_string(),
    ];

    if let Some(speaker) = &config.speaker {
        args.push("--speaker".to_string());
        args.push(speaker.clone());
    }

    args
}

impl StreamSynthesizeSpeech for PiperSpeech {
    fn synthesize_stream(&self, text: &str) -> Result<PcmStream, SpeechError> {
        let mut child = Command::new(&self.config.bin_path)
            .args(build_args(&self.config))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| SpeechError::Unavailable(format!("could not start Piper: {error}")))?;

        let mut stdin = child
            .stdin
            .take()
            .expect("stdin was piped when the child was spawned");
        let stdout = child
            .stdout
            .take()
            .expect("stdout was piped when the child was spawned");

        // Piper reads its whole input, then generates - it doesn't need
        // this write on its own thread the way a network call would, but
        // writing before taking stdout keeps ordering obvious. Dropping
        // `stdin` (end of scope) signals EOF, which is what makes Piper
        // start synthesizing.
        stdin
            .write_all(normalize_text(text).as_bytes())
            .map_err(|error| {
                SpeechError::Synthesis(format!("failed to send text to Piper: {error}"))
            })?;
        drop(stdin);

        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            stream_pcm_from(stdout, &sender, "Piper");

            // Surface a non-zero exit (e.g. a bad --speaker index) even
            // though sample data may already have been sent - a caller
            // that got partial audio still deserves to know Piper failed.
            match child.wait() {
                Ok(status) if !status.success() => {
                    let stderr = child.stderr.take().map(read_all_lossy).unwrap_or_default();
                    let _ = sender.send(Err(SpeechError::Synthesis(format!(
                        "Piper exited with {status}: {}",
                        stderr.trim()
                    ))));
                }
                Ok(_) => {}
                Err(error) => {
                    let _ = sender.send(Err(SpeechError::Synthesis(format!(
                        "failed to wait for Piper: {error}"
                    ))));
                }
            }
        });

        Ok(PcmStream {
            sample_rate: self.config.sample_rate,
            channels: 1,
            chunks: receiver,
        })
    }
}

fn read_all_lossy(mut reader: impl std::io::Read) -> String {
    let mut buffer = String::new();
    let _ = std::io::Read::read_to_string(&mut reader, &mut buffer);
    buffer
}
