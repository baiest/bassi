# Guía: STT (voz → texto) para Nala, paso a paso

Vas a construir `crates/stt`, simétrico a `crates/tts` (que ya existe). No
tocás `apps/nala` en ningún paso — solo `crates/stt` (nuevo) y
`apps/voice/src/main.rs`.

Motor elegido: **cpal** (captura de mic) + **whisper-rs** (audio → texto,
corre local, sin nube). Modo de activación: **push-to-talk por Enter**
(Enter para empezar a grabar, Enter de nuevo para parar) — no hay wake-word
ni VAD en esta guía, eso queda para después.

Seguí las fases en orden. Cada una termina compilando y con tests en verde
antes de pasar a la siguiente.

---

## Fase 0 — Ticket y rama

Igual que siempre (`CONTRIBUTING.md` / skill `bassi-workflow`):

1. Crear card en Trello, ej. `BAS-<id>: STT push-to-talk para apps/voice`.
2. `git checkout main && git pull && git checkout -b BAS-<id>-stt-push-to-talk`

---

## Fase 1 — Crear `crates/stt` y capturar audio del mic

### 1.1 Estructura de carpetas

```
crates/stt/
  Cargo.toml
  src/
    lib.rs
    capture.rs
  tests/
    (vacío por ahora, llega en Fase 2)
```

### 1.2 `crates/stt/Cargo.toml`

```toml
[package]
name = "stt"
version = "0.1.0"
edition = "2024"

[dependencies]
cpal = "0.15"
hound = "3"
thiserror = { workspace = true }
```

`hound` es para leer/escribir `.wav` — lo usás para guardar lo que grabás y
poder escucharlo, y más adelante para los fixtures de test.

### 1.3 Agregar el crate al workspace

En el `Cargo.toml` de la raíz, agregá `"crates/stt"` a `members`:

```toml
members = [
    "apps/nala",
    "apps/voice",
    "crates/mcp",
    "crates/tts",
    "crates/stt",
    "crates/process-group",
]
```

### 1.4 `crates/stt/src/lib.rs`

```rust
//! Speech-to-text: captura de micrófono y transcripción. Sin dependencia
//! de `nala` — cualquier consumidor de este crate lo puede usar solo.

mod capture;

pub use capture::{record_until_enter, RecordedAudio, CaptureError};
```

### 1.5 `crates/stt/src/capture.rs`

Esto graba desde el micrófono hasta que el usuario aprieta Enter. Whisper
necesita audio mono a 16 kHz, así que convertís ahí mismo.

```rust
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub const WHISPER_SAMPLE_RATE: u32 = 16_000;

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("no input device found")]
    NoInputDevice,
    #[error("failed to build input stream: {0}")]
    StreamBuild(String),
    #[error("failed to start recording: {0}")]
    StreamStart(String),
}

/// Audio grabado, ya en mono 16-bit a `WHISPER_SAMPLE_RATE` — listo para
/// pasarle directo a whisper-rs sin resamplear de nuevo.
pub struct RecordedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

/// Graba del micrófono por defecto hasta que el caller llama a `stop`
/// (acá: hasta que el usuario aprieta Enter en la consola). Bloqueante:
/// no vuelve hasta que termina la grabación.
pub fn record_until_enter() -> Result<RecordedAudio, CaptureError> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or(CaptureError::NoInputDevice)?;
    let config = device
        .default_input_config()
        .map_err(|e| CaptureError::StreamBuild(e.to_string()))?;

    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
    let buffer_writer = Arc::clone(&buffer);

    let stream = device
        .build_input_stream(
            &config.into(),
            move |data: &[f32], _| {
                let mut buffer = buffer_writer.lock().unwrap();
                // Mezcla a mono promediando canales, si hay más de uno.
                for frame in data.chunks(channels) {
                    let mono: f32 = frame.iter().sum::<f32>() / channels as f32;
                    buffer.push(mono);
                }
            },
            |err| eprintln!("Audio input error: {err}"),
            None,
        )
        .map_err(|e| CaptureError::StreamBuild(e.to_string()))?;

    stream
        .play()
        .map_err(|e| CaptureError::StreamStart(e.to_string()))?;

    println!("Grabando... apretá Enter para terminar.");
    let mut discard = String::new();
    std::io::stdin().read_line(&mut discard).ok();

    drop(stream);

    let raw_samples = buffer.lock().unwrap().clone();
    let resampled = resample_linear(&raw_samples, sample_rate, WHISPER_SAMPLE_RATE);

    Ok(RecordedAudio {
        samples: resampled,
        sample_rate: WHISPER_SAMPLE_RATE,
    })
}

/// Resampleo lineal simple. No es de calidad de estudio, pero alcanza y
/// sobra para STT — whisper.cpp ya es tolerante a algo de ruido/artefactos.
fn resample_linear(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = (samples.len() as f64 / ratio) as usize;
    (0..out_len)
        .map(|i| {
            let src_index = i as f64 * ratio;
            let lower = src_index.floor() as usize;
            let upper = (lower + 1).min(samples.len() - 1);
            let frac = (src_index - lower as f64) as f32;
            samples[lower] * (1.0 - frac) + samples[upper] * frac
        })
        .collect()
}
```

### 1.6 Probarlo manualmente (antes de seguir)

Creá un ejemplo rápido para escuchar lo que grabaste — esto NO es parte del
crate final, es solo para validar que el mic anda:

`crates/stt/examples/record_and_save.rs`:

```rust
fn main() {
    let audio = stt::record_until_enter().expect("recording failed");

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: audio.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create("test_recording.wav", spec).unwrap();
    for sample in audio.samples {
        writer.write_sample((sample * i16::MAX as f32) as i16).unwrap();
    }
    writer.finalize().unwrap();
    println!("Guardado en test_recording.wav");
}
```

Correlo:

```bash
cargo run -p stt --example record_and_save
```

Hablá, apretá Enter, y escuchá `test_recording.wav`. **No sigas a la Fase 2
hasta que esto suene bien.**

**Criterio de éxito Fase 1:** el `.wav` grabado suena como tu voz, sin
cortes raros ni silencio total.

---

## Fase 2 — Transcripción con whisper-rs

### 2.1 Descargar el modelo

Whisper.cpp usa modelos `.bin` (formato GGML). Descargá uno chico para
empezar (`base` anda bien en español, `small` es más preciso pero más
lento):

```bash
mkdir -p data/whisper
curl -L -o data/whisper/ggml-base.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin
```

(Podés cambiar `base` por `small` o `tiny` en la URL según cuánta CPU
tengas — `tiny` es el más rápido pero menos preciso.)

### 2.2 Agregar `whisper-rs` a `crates/stt/Cargo.toml`

```toml
[dependencies]
cpal = "0.15"
hound = "3"
thiserror = { workspace = true }
whisper-rs = "0.12"
```

`whisper-rs` compila el C++ de whisper.cpp — la primera build va a tardar
varios minutos. Necesitás un compilador de C++ instalado (en Windows, las
build tools de Visual Studio; normalmente ya las tenés si compilás Rust ahí).

### 2.3 `crates/stt/src/transcribe.rs`

```rust
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

#[derive(Debug, thiserror::Error)]
pub enum TranscribeError {
    #[error("failed to load whisper model at '{0}': {1}")]
    ModelLoad(String, String),
    #[error("transcription failed: {0}")]
    Transcription(String),
}

pub struct Transcriber {
    context: WhisperContext,
}

impl Transcriber {
    pub fn load(model_path: &str) -> Result<Self, TranscribeError> {
        let context = WhisperContext::new_with_params(
            model_path,
            WhisperContextParameters::default(),
        )
        .map_err(|e| TranscribeError::ModelLoad(model_path.to_string(), e.to_string()))?;

        Ok(Self { context })
    }

    /// `samples` debe ser mono, 16 kHz — exactamente lo que devuelve
    /// `record_until_enter`.
    pub fn transcribe(&self, samples: &[f32]) -> Result<String, TranscribeError> {
        let mut state = self
            .context
            .create_state()
            .map_err(|e| TranscribeError::Transcription(e.to_string()))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("es"));
        params.set_print_progress(false);
        params.set_print_special(false);
        params.set_print_realtime(false);

        state
            .full(params, samples)
            .map_err(|e| TranscribeError::Transcription(e.to_string()))?;

        let num_segments = state
            .full_n_segments()
            .map_err(|e| TranscribeError::Transcription(e.to_string()))?;

        let mut text = String::new();
        for i in 0..num_segments {
            if let Ok(segment) = state.full_get_segment_text(i) {
                text.push_str(&segment);
            }
        }

        Ok(text.trim().to_string())
    }
}
```

Actualizá `crates/stt/src/lib.rs`:

```rust
mod capture;
mod transcribe;

pub use capture::{record_until_enter, CaptureError, RecordedAudio, WHISPER_SAMPLE_RATE};
pub use transcribe::{Transcriber, TranscribeError};
```

### 2.4 Test con un fixture (no con el mic real)

Igual que `crates/tts/tests/fixtures/piper/`, necesitás un `.wav` fijo para
testear sin depender de audio en vivo. Grabate diciendo algo simple
("hola mundo") con el ejemplo de la Fase 1, y copiá ese archivo a:

```
crates/stt/tests/fixtures/hola_mundo.wav
```

`crates/stt/tests/transcribe.rs`:

```rust
use stt::Transcriber;

#[test]
#[ignore] // requiere el modelo descargado — correlo a mano con --ignored
fn transcribes_a_known_recording() {
    let transcriber = Transcriber::load("data/whisper/ggml-base.bin")
        .expect("model should load");

    let mut reader = hound::WavReader::open("tests/fixtures/hola_mundo.wav")
        .expect("fixture should exist");
    let samples: Vec<f32> = reader
        .samples::<i16>()
        .map(|s| s.unwrap() as f32 / i16::MAX as f32)
        .collect();

    let text = transcriber.transcribe(&samples).unwrap();

    assert!(text.to_lowercase().contains("hola"));
}
```

Corré:

```bash
cargo test -p stt -- --ignored
```

Está marcado `#[ignore]` porque el modelo (`.bin`) no debería vivir en el
repo ni descargarse en cada `cargo test` normal — igual que Piper/Chatterbox
reales quedan fuera de `cargo test` por defecto (ver
`scripts/check_coverage.sh`).

**Criterio de éxito Fase 2:** el test imprime/confirma que "hola" aparece
en la transcripción.

---

## Fase 3 — Conectarlo a `apps/voice`

### 3.1 Agregar dependencia

En `apps/voice/Cargo.toml`:

```toml
[dependencies]
nala = { path = "../nala" }
tts = { path = "../../crates/tts" }
stt = { path = "../../crates/stt" }
```

### 3.2 Cargar el modelo en `apps/voice/src/bootstrap.rs`

Agregá una función que construya el `Transcriber` una sola vez (cargar el
modelo es lento, no lo hagas por turno):

```rust
pub fn build_transcriber() -> stt::Transcriber {
    let model_path = std::env::var("NALA_WHISPER_MODEL")
        .unwrap_or_else(|_| "data/whisper/ggml-base.bin".to_string());

    stt::Transcriber::load(&model_path).expect("Failed to load Whisper model")
}
```

### 3.3 Reemplazar el lector en `apps/voice/src/main.rs`

Hoy tenés:

```rust
let input = match reader.read().expect("Failed reading input") {
    Some(input) => input,
    None => break,
};
```

Cambialo por (dejá el teclado como fallback si querés seguir debuggeando
por texto — podés alternar con una env var):

```rust
let transcriber = voice::bootstrap::build_transcriber();

// dentro del loop:
println!("Apretá Enter para hablar...");
let mut trigger = String::new();
std::io::stdin().read_line(&mut trigger).ok();

let audio = match stt::record_until_enter() {
    Ok(audio) => audio,
    Err(e) => {
        eprintln!("Error grabando: {e}");
        continue;
    }
};

let input = match transcriber.transcribe(&audio.samples) {
    Ok(text) if !text.trim().is_empty() => text,
    Ok(_) => {
        println!("No se entendió nada, probá de nuevo.");
        continue;
    }
    Err(e) => {
        eprintln!("Error transcribiendo: {e}");
        continue;
    }
};

println!("Vos: {input}");
```

El resto del loop (`assistant.process(&input)`, `speech.say(...)`) queda
exactamente igual — no lo toques.

### 3.4 Probar todo el flujo

```bash
cargo run -p voice
```

Enter → hablás → Enter → Nala te contesta.

**Criterio de éxito Fase 3:** le hablás en voz alta y Nala responde, sin
tocar el teclado salvo para marcar inicio/fin de grabación.

---

## Fase 4 — Limpieza y PR

1. `cargo fmt --all`
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings`
3. `cargo test --workspace` (el test `#[ignore]` de Fase 2 no corre acá,
   está bien)
4. `bash scripts/check_rust.sh`
5. Commit siguiendo el formato de siempre, PR contra `main` referenciando
   tu ticket.

No agregues el modelo `.bin` al repo (pesa demasiado) — sumá `data/whisper/`
a `.gitignore` si no está ya.

---

## Qué queda para después (no lo hagas todavía)

- **VAD** (cortar la grabación sola por silencio, sin el segundo Enter).
- **Wake-word** ("Hey Nala" sin tocar nada).
- **Barge-in** (poder interrumpir a Nala hablando mientras ella habla).

Todo eso es iteración sobre lo que armaste acá, no un rediseño — cuando
llegue el momento, el lugar donde entra es siempre el mismo:
`apps/voice/src/main.rs`, reemplazando cómo decidís que empezó/terminó un
turno de audio.

---

## Estado (2026-08-31): VAD + wake word implementados, pero desconectados

La rama `BAS-25-always-listening-vad-wakeword` terminó de construir todo lo
de "Qué queda para después": VAD (Silero, `crates/stt/src/vad.rs`), wake word
("oye Nala" y variantes, `crates/stt/src/wake.rs`), la máquina de estados de
sesión (`crates/stt/src/session.rs`) y el `Listener` que las une
(`crates/stt/src/listener.rs`). Todo compila y tiene tests
(`crates/stt/tests/vad.rs`, tests unitarios en cada módulo).

Por ahora queda **desconectado** de `apps/voice`: en el uso diario, la latencia
de la verificación de wake word y los falsos disparos hacían más difícil de
usar que el push-to-talk simple. `apps/voice/src/main.rs` y
`apps/voice/src/bootstrap.rs` volvieron al flujo de la Fase 3 de esta guía
(Enter → grabar → Enter → transcribir), tal cual estaba en `main` antes de la
rama.

Para reactivarlo cuando se retome: recuperar `build_listener()` (borrado de
`bootstrap.rs`, pero existe en el historial de esta rama hasta el commit
`b658fe1`) y volver a cablear `main.rs` para usar `Listener::listen` con
`ListenMode` en vez de `record_until_enter`. Ver BAS-25.
