//! Covers `handle_connection`'s greeting emission over a real socket — the
//! FakeWire-based tests in `tests/server.rs` build an `Assistant` directly
//! and never go through `handle_connection`, so this is the only place
//! this behavior is exercised.

use std::sync::Arc;
use std::thread;

use agent_protocol::{Event, ServerMessage};
use nala::application::devices::registry::DeviceRegistry;
use nala::device_server::Device;
use nala::server::serve;
use tungstenite::Message;

#[test]
fn a_connecting_client_receives_the_greeting_before_anything_else() {
    let devices: Arc<DeviceRegistry<Device>> = Arc::new(DeviceRegistry::new());
    // Port 0 isn't resolvable up-front here since `serve` binds
    // internally; pick a high, likely-free ephemeral port instead.
    let addr = "127.0.0.1:41824".to_string();
    let server_addr = addr.clone();
    thread::spawn(move || {
        serve(&server_addr, devices).expect("nala server should bind");
    });

    let mut client = connect(&addr);

    match read_message(&mut client) {
        ServerMessage::Event(Event::Greeting { text }) => assert!(!text.is_empty()),
        other => panic!("expected the greeting first, got {other:?}"),
    }
}

fn connect(
    addr: &str,
) -> tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>> {
    for _ in 0..50 {
        if let Ok((ws, _)) = tungstenite::connect(format!("ws://{addr}")) {
            return ws;
        }
        thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("could not connect to the nala server");
}

fn read_message(
    ws: &mut tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
) -> ServerMessage {
    loop {
        match ws.read().expect("read a message") {
            Message::Text(text) => {
                return serde_json::from_str(&text).expect("valid ServerMessage");
            }
            _ => continue,
        }
    }
}
