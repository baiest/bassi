use nala::adapters::speech::chatterbox::{HttpChatterbox, SynthesizeSpeech};
use nala::ports::speech::SpeechError;

use crate::http_stub::HttpStub;

fn client(base_url: &str) -> HttpChatterbox {
    HttpChatterbox::new(
        base_url,
        "nala",
        "es",
        0.5,
        0.5,
        0.8,
        std::time::Duration::from_secs(5),
    )
}

#[test]
fn http_posts_expected_payload() {
    let stub = HttpStub::start(200, "fake-wav-bytes");

    let result = client(&stub.base_url).synthesize("Hola, como estas?");

    assert!(result.is_ok());

    let body = stub.received_body();
    let json: serde_json::Value = serde_json::from_str(&body).expect("body should be JSON");
    assert_eq!(json["input"], "Hola, como estas?");
    assert_eq!(json["voice"], "nala");
    assert_eq!(json["language"], "es");
}

#[test]
fn http_returns_body_bytes_on_success() {
    let stub = HttpStub::start(200, "fake-wav-bytes");

    let bytes = client(&stub.base_url).synthesize("hola").unwrap();

    assert_eq!(bytes, b"fake-wav-bytes".to_vec());
}

#[test]
fn http_maps_server_error_to_unavailable() {
    let stub = HttpStub::start(500, "boom");

    let error = client(&stub.base_url).synthesize("hola").unwrap_err();

    assert!(matches!(error, SpeechError::Unavailable(_)));
}

#[test]
fn http_maps_bad_request_to_synthesis_error() {
    let stub = HttpStub::start(400, "bad voice");

    let error = client(&stub.base_url).synthesize("hola").unwrap_err();

    assert!(matches!(error, SpeechError::Synthesis(_)));
}

#[test]
fn http_maps_empty_body_to_synthesis_error() {
    let stub = HttpStub::start(200, "");

    let error = client(&stub.base_url).synthesize("hola").unwrap_err();

    assert!(matches!(error, SpeechError::Synthesis(_)));
}

#[test]
fn http_connection_refused_is_unavailable() {
    // Port 0 never accepts connections; nothing is listening there.
    let error = client("http://127.0.0.1:1").synthesize("hola").unwrap_err();

    assert!(matches!(error, SpeechError::Unavailable(_)));
}
