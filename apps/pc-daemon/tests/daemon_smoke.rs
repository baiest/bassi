//! One end-to-end `Invoke` over a real `TcpListener`/`tungstenite` socket,
//! to cover the wire-level `TcpDeviceWire` that the fake-`DeviceWire` tests
//! in `daemon.rs` don't exercise — mirrors `apps/nala/tests/server_smoke.rs`.

#[path = "common/fake_computer.rs"]
mod fake_computer;

use std::net::TcpListener;
use std::thread;

use device_capabilities::capabilities::execute_command::ExecuteCommandTool;
use device_capabilities::registry::CapabilityRegistry;
use device_protocol::{DeviceMessage, NalaMessage, Outcome};
use fake_computer::FakeComputer;
use pc_daemon::client::TcpDeviceWire;
use pc_daemon::config::DeviceIdentity;
use pc_daemon::daemon::{SessionOutcome, run_session};
use pc_daemon::overlay_channel::OverlayChannel;

#[test]
fn a_daemon_can_complete_one_invoke_over_a_real_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port");
    let addr = listener.local_addr().expect("local addr");

    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept a connection");
        let mut ws = tungstenite::accept(stream).expect("complete the WS handshake");

        // Read the daemon's Hello, then send it one Invoke and read back the
        // Result — enough to prove the real socket carries a full round
        // trip, without reimplementing Nala's device_server here.
        let hello: DeviceMessage = loop {
            match ws.read().expect("read Hello") {
                tungstenite::Message::Text(text) => {
                    break serde_json::from_str(&text).expect("valid DeviceMessage");
                }
                _ => continue,
            }
        };
        assert!(matches!(hello, DeviceMessage::Hello { .. }));

        let invoke = NalaMessage::Invoke {
            request_id: "req-1".to_string(),
            capability: "execute_command".to_string(),
            arguments: r#"{"command":"start chrome"}"#.to_string(),
        };
        let json = serde_json::to_string(&invoke).expect("serialize Invoke");
        ws.send(tungstenite::Message::Text(json))
            .expect("send Invoke");

        let result: DeviceMessage = loop {
            match ws.read().expect("read Result") {
                tungstenite::Message::Text(text) => {
                    break serde_json::from_str(&text).expect("valid DeviceMessage");
                }
                _ => continue,
            }
        };

        ws.close(None).ok();
        result
    });

    let mut wire = TcpDeviceWire::connect(&addr.to_string()).expect("connect to the daemon side");
    let mut registry = CapabilityRegistry::new();
    registry.register(ExecuteCommandTool::new(FakeComputer::new()));
    let identity = DeviceIdentity {
        device_id: "pc-1".to_string(),
        name: "pc".to_string(),
        platform: "windows".to_string(),
        token: "secret".to_string(),
    };

    let overlay = OverlayChannel::new();
    let outcome = run_session(&mut wire, &mut registry, &identity, &overlay)
        .expect("session should not error");
    assert!(matches!(outcome, SessionOutcome::Closed));

    let result = server.join().expect("server thread should not panic");
    match result {
        DeviceMessage::Result {
            request_id,
            outcome,
        } => {
            assert_eq!(request_id, "req-1");
            assert!(matches!(outcome, Outcome::Ok { .. }));
        }
        other => panic!("expected Result, got {other:?}"),
    }
}
