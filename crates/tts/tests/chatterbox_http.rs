#[path = "common/http_stub.rs"]
mod http_stub;

use http_stub::HttpStub;
use tts::{SpeechError, StreamSynthesizeSpeech, chatterbox::HttpChatterbox};

fn client(base_url: &str) -> HttpChatterbox {
    HttpChatterbox::new(
        base_url,
        "nala",
        0.5,
        0.5,
        0.8,
        "sentence",
        200,
        std::time::Duration::from_secs(5),
        std::time::Duration::from_secs(5),
    )
}

/// Builds a minimal canonical PCM WAV: `RIFF`/`WAVE`/`fmt `/`data` chunks,
/// 16-bit little-endian samples, matching what `chatterbox-tts-api` streams
/// (see `docs/STREAMING_API.md` and `app/api/endpoints/speech.py`).
fn wav_bytes(sample_rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
    let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    let byte_rate = sample_rate * channels as u32 * 2;
    let block_align = channels * 2;

    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&byte_rate.to_le_bytes());
    bytes.extend_from_slice(&block_align.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&data);
    bytes
}

fn drain(receiver: std::sync::mpsc::Receiver<Result<Vec<i16>, SpeechError>>) -> Vec<i16> {
    receiver.into_iter().flat_map(|c| c.unwrap()).collect()
}

#[test]
fn http_posts_expected_payload() {
    let stub = HttpStub::start_bytes(200, wav_bytes(24_000, 1, &[1, 2, 3]));

    let result = client(&stub.base_url).synthesize_stream("Hola, como estas?");

    assert!(result.is_ok());

    let body = stub.received_body();
    let json: serde_json::Value = serde_json::from_str(&body).expect("body should be JSON");
    assert_eq!(json["input"], "Hola, como estas?");
    assert_eq!(json["voice"], "nala");
    assert_eq!(json["streaming_strategy"], "sentence");
    assert_eq!(json["streaming_chunk_size"], 200);
}

#[test]
fn http_streams_header_and_samples_on_success() {
    let stub = HttpStub::start_bytes(200, wav_bytes(24_000, 1, &[1, 2, 3, -4]));

    let stream = client(&stub.base_url)
        .synthesize_stream("hola")
        .expect("stream should start");

    assert_eq!(stream.sample_rate, 24_000);
    assert_eq!(stream.channels, 1);
    assert_eq!(drain(stream.chunks), vec![1, 2, 3, -4]);
}

#[test]
fn http_maps_server_error_to_unavailable() {
    let stub = HttpStub::start(500, "boom");

    let error = client(&stub.base_url)
        .synthesize_stream("hola")
        .unwrap_err();

    assert!(matches!(error, SpeechError::Unavailable(_)));
}

#[test]
fn http_maps_bad_request_to_synthesis_error() {
    let stub = HttpStub::start(400, "bad voice");

    let error = client(&stub.base_url)
        .synthesize_stream("hola")
        .unwrap_err();

    assert!(matches!(error, SpeechError::Synthesis(_)));
}

#[test]
fn http_maps_non_wav_body_to_synthesis_error() {
    let stub = HttpStub::start(200, "not a wav file at all, way over 44 bytes long");

    let stream = client(&stub.base_url)
        .synthesize_stream("hola")
        .expect_err("a body without a valid WAV header should fail up front");

    assert!(matches!(stream, SpeechError::Synthesis(_)));
}

#[test]
fn http_maps_empty_stream_to_synthesis_error() {
    let stub = HttpStub::start_bytes(200, wav_bytes(24_000, 1, &[]));

    let stream = client(&stub.base_url)
        .synthesize_stream("hola")
        .expect("header-only response still starts a stream");

    let error = stream
        .chunks
        .recv()
        .expect("stream should end with an error");

    assert!(matches!(error, Err(SpeechError::Synthesis(_))));
}

#[test]
fn http_connection_refused_is_unavailable() {
    // Port 1 never accepts connections; nothing is listening there.
    let error = client("http://127.0.0.1:1")
        .synthesize_stream("hola")
        .unwrap_err();

    assert!(matches!(error, SpeechError::Unavailable(_)));
}
