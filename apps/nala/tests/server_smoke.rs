//! One end-to-end turn over a real `TcpListener`/`tungstenite` socket, to
//! cover the wire-level `Wire for WebSocket<S>` impl that the fake-`Wire`
//! tests in `server.rs` don't exercise.

#[path = "common/fake_llm.rs"]
#[allow(dead_code)]
mod fake_llm;

#[path = "common/fake_computer.rs"]
#[allow(dead_code)]
mod fake_computer;

#[path = "common/fake_events.rs"]
#[allow(dead_code)]
mod fake_events;

use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

use agent_protocol::{ClientMessage, ServerMessage};
use device_capabilities::Capability;
use device_capabilities::capabilities::execute_command::ExecuteCommandTool;
use fake_computer::FakeComputer;
use fake_events::RecordingEventSink;
use fake_llm::AlwaysRepliesTextLlm;
use nala::application::assistant::Assistant;
use nala::application::tools::dispatcher::{ToolDispatcher, Tools};
use nala::application::tools::registry::ToolRegistry;
use nala::server::{WsEventSink, run_session};
use tungstenite::Message;

#[test]
fn a_client_can_complete_one_turn_over_a_real_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral port");
    let addr = listener.local_addr().expect("local addr");

    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept a connection");
        let ws = tungstenite::accept(stream).expect("complete the WS handshake");
        let wire = Arc::new(Mutex::new(ws));

        let mut registry = ToolRegistry::new();
        registry.register(ExecuteCommandTool::<FakeComputer>::definition().into());
        let mut dispatcher = ToolDispatcher::<FakeComputer>::new();
        dispatcher.register(Tools::ExecuteCommand(ExecuteCommandTool::new(
            FakeComputer::new(),
        )));

        let assistant = Assistant::new(
            AlwaysRepliesTextLlm::new(),
            dispatcher,
            registry,
            WsEventSink::new(RecordingEventSink::new(), Arc::clone(&wire)),
        );

        run_session(assistant, wire);
    });

    let (mut client, _) =
        tungstenite::connect(format!("ws://{addr}")).expect("connect as a client");

    let input = serde_json::to_string(&ClientMessage::Input {
        text: "hola".to_string(),
        source: agent_protocol::RequestSource::Cli,
    })
    .unwrap();
    client.send(Message::Text(input)).unwrap();

    let reply = loop {
        match client.read().expect("read a server message") {
            Message::Text(text) => {
                let message: ServerMessage = serde_json::from_str(&text).unwrap();
                if let ServerMessage::Reply { text } = message {
                    break text;
                }
            }
            _ => continue,
        }
    };
    assert_eq!(reply, "ok");

    client.close(None).ok();
    server.join().expect("server thread should not panic");
}
