//! One end-to-end connection over a real `TcpListener`/`tungstenite` socket,
//! covering `device_server::serve` itself — the `Greeting` sent right after
//! `Welcome` in particular, which the inline unit tests in `device_server.rs`
//! don't exercise since they stop at `validate_hello`.

use std::net::TcpStream;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use device_protocol::{DeviceMessage, NalaMessage, PROTOCOL_VERSION};
use nala::application::devices::registry::DeviceRegistry;
use nala::device_server::{self, Device};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

fn connect(addr: &str) -> WebSocket<MaybeTlsStream<TcpStream>> {
    for _ in 0..50 {
        if let Ok((ws, _)) = tungstenite::connect(format!("ws://{addr}")) {
            return ws;
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!("could not connect to the device server");
}

fn read_message(ws: &mut WebSocket<MaybeTlsStream<TcpStream>>) -> NalaMessage {
    loop {
        match ws.read().expect("read a message") {
            Message::Text(text) => return serde_json::from_str(&text).expect("valid NalaMessage"),
            _ => continue,
        }
    }
}

#[test]
fn a_connecting_device_receives_a_greeting_right_after_welcome() {
    let registry = Arc::new(DeviceRegistry::<Device>::new());
    // Port 0 isn't resolvable up-front here since `serve` binds internally;
    // pick a high, likely-free ephemeral port instead.
    let addr = "127.0.0.1:41823".to_string();
    let server_addr = addr.clone();
    thread::spawn(move || {
        device_server::serve(&server_addr, registry, Some("secret".to_string()))
            .expect("device server should bind");
    });

    let mut ws = connect(&addr);
    let hello = DeviceMessage::Hello {
        protocol_version: PROTOCOL_VERSION,
        device_id: "pc-1".to_string(),
        name: "pc".to_string(),
        platform: "windows".to_string(),
        token: "secret".to_string(),
        capabilities: vec![],
    };
    ws.send(Message::Text(serde_json::to_string(&hello).unwrap()))
        .expect("send Hello");

    assert!(matches!(read_message(&mut ws), NalaMessage::Welcome { .. }));
    match read_message(&mut ws) {
        NalaMessage::Greeting { text } => assert!(!text.is_empty()),
        other => panic!("expected Greeting right after Welcome, got {other:?}"),
    }
}
