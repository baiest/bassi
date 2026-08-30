//! Live diagnostic for the always-listening pipeline: prints the VAD's
//! speech probability for every chunk, and periodically transcribes the
//! last second of audio so you can see exactly what Whisper heard.
//!
//! Run with:
//!   cargo run -p stt --example diagnose
//!
//! Set NALA_WHISPER_MODEL to override the model (defaults to base, same
//! as apps/voice).

use cpal::traits::{DeviceTrait, HostTrait};
use stt::{AudioSource, CHUNK_SAMPLES, MicStream, SileroVad, Transcriber, VoiceDetector};

/// Lists every input device the system reports, marking which one cpal
/// (and therefore `MicStream`) will actually open by default.
fn print_input_devices() {
    let host = cpal::default_host();
    println!("Host de audio: {:?}", host.id());

    let default_name = host
        .default_input_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_else(|| "<ninguno>".to_string());
    println!("Micrófono por defecto: {default_name}");

    match host.input_devices() {
        Ok(devices) => {
            println!("Dispositivos de entrada disponibles:");
            for device in devices {
                let name = device.name().unwrap_or_else(|_| "<sin nombre>".to_string());
                let marker = if name == default_name {
                    " (por defecto)"
                } else {
                    ""
                };
                let config = device
                    .default_input_config()
                    .map(|c| format!("{} Hz, {} canal(es)", c.sample_rate().0, c.channels()))
                    .unwrap_or_else(|_| "config no disponible".to_string());
                println!("  - {name}{marker} — {config}");
            }
        }
        Err(e) => eprintln!("No se pudieron listar los dispositivos: {e}"),
    }
    println!();
}

fn main() {
    print_input_devices();

    let model_path = std::env::var("NALA_WHISPER_MODEL")
        .unwrap_or_else(|_| "data/whisper/ggml-base.bin".to_string());

    println!("Cargando modelo Whisper desde '{model_path}'...");
    let transcriber = Transcriber::load(&model_path).expect(
        "No se pudo cargar el modelo. Verificá NALA_WHISPER_MODEL o corré scripts/stt-setup.ps1.",
    );

    println!("Abriendo el micrófono...");
    let mut audio = MicStream::open().expect(
        "No se pudo abrir el micrófono. Verificá que haya un dispositivo de entrada por defecto.",
    );

    let mut vad = SileroVad::new().expect("No se pudo crear el detector de voz (VAD).");

    println!("Listo. Hablá — vas a ver la probabilidad de voz en cada chunk (32ms).");
    println!("Cada ~1s se transcribe el último segundo de audio con Whisper.");
    println!("Ctrl+C para salir.\n");

    let mut chunk = [0.0_f32; CHUNK_SAMPLES];
    let mut recent_second: Vec<f32> = Vec::new();
    let mut chunks_since_transcribe = 0;
    const CHUNKS_PER_SECOND: usize = 31; // 1000ms / 32ms

    loop {
        if !audio.next_chunk(&mut chunk) {
            eprintln!("El micrófono se cerró inesperadamente.");
            break;
        }

        let probability = vad.probability(&chunk);
        let bar_len = (probability * 40.0) as usize;
        let bar: String = "#".repeat(bar_len);
        let marker = if probability > 0.5 { "VOZ" } else { "   " };
        println!("{marker} [{bar:<40}] {probability:.2}");

        recent_second.extend_from_slice(&chunk);
        chunks_since_transcribe += 1;

        if chunks_since_transcribe >= CHUNKS_PER_SECOND {
            chunks_since_transcribe = 0;
            match transcriber.transcribe(&recent_second) {
                Ok(text) if !text.trim().is_empty() => {
                    println!(">>> Whisper escuchó: \"{text}\"");
                }
                Ok(_) => {}
                Err(e) => eprintln!(">>> Error transcribiendo: {e}"),
            }
            recent_second.clear();
        }
    }
}
