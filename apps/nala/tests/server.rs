#[path = "common/fake_llm.rs"]
#[allow(dead_code)]
mod fake_llm;

#[path = "common/fake_computer.rs"]
#[allow(dead_code)]
mod fake_computer;

#[path = "common/fake_events.rs"]
#[allow(dead_code)]
mod fake_events;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use agent_protocol::{ClientMessage, ServerMessage};
use device_capabilities::Capability;
use device_capabilities::capabilities::execute_command::ExecuteCommandTool;
use fake_computer::FakeComputer;
use fake_events::RecordingEventSink;
use fake_llm::{AlwaysRepliesTextLlm, FailingLlm};
use nala::application::assistant::Assistant;
use nala::application::tools::dispatcher::{ToolDispatcher, Tools};
use nala::application::tools::registry::ToolRegistry;
use nala::server::{Wire, WsEventSink, run_session};

/// An in-memory `Wire`: `recv()` pops scripted client messages (`None` once
/// exhausted, simulating the client disconnecting) and `send()` records
/// every server message it was asked to send, in order.
struct FakeWire {
    incoming: VecDeque<ClientMessage>,
    sent: Vec<ServerMessage>,
}

impl FakeWire {
    fn new(incoming: Vec<ClientMessage>) -> Self {
        Self {
            incoming: incoming.into(),
            sent: Vec::new(),
        }
    }
}

impl Wire for FakeWire {
    fn recv(&mut self) -> std::io::Result<Option<ClientMessage>> {
        Ok(self.incoming.pop_front())
    }

    fn send(&mut self, message: ServerMessage) -> std::io::Result<()> {
        self.sent.push(message);
        Ok(())
    }
}

fn assistant_with<L>(
    llm: L,
    wire: Arc<Mutex<FakeWire>>,
) -> Assistant<L, ToolDispatcher<FakeComputer>, WsEventSink<RecordingEventSink, FakeWire>>
where
    L: nala::ports::llm::Llm + Send + 'static,
{
    let mut registry = ToolRegistry::new();
    registry.register(ExecuteCommandTool::<FakeComputer>::definition().into());

    let mut dispatcher = ToolDispatcher::new();
    dispatcher.register(Tools::ExecuteCommand(ExecuteCommandTool::new(
        FakeComputer::new(),
    )));

    Assistant::new(
        llm,
        dispatcher,
        registry,
        WsEventSink::new(RecordingEventSink::new(), wire),
    )
}

fn only_reply_and_event_messages(sent: &[ServerMessage]) -> (usize, usize) {
    let events = sent
        .iter()
        .filter(|message| matches!(message, ServerMessage::Event(_)))
        .count();
    let replies = sent
        .iter()
        .filter(|message| matches!(message, ServerMessage::Reply { .. }))
        .count();
    (events, replies)
}

#[test]
fn an_input_message_produces_a_reply_from_the_agent() {
    let wire = Arc::new(Mutex::new(FakeWire::new(vec![ClientMessage::Input {
        text: "hola".to_string(),
    }])));
    let assistant = assistant_with(AlwaysRepliesTextLlm::new(), Arc::clone(&wire));

    run_session(assistant, Arc::clone(&wire));

    let sent = wire.lock().unwrap().sent.clone();
    assert!(matches!(
        sent.last(),
        Some(ServerMessage::Reply { text }) if text == "ok"
    ));
}

#[test]
fn progress_events_are_sent_before_the_final_reply() {
    let wire = Arc::new(Mutex::new(FakeWire::new(vec![ClientMessage::Input {
        text: "hola".to_string(),
    }])));
    let assistant = assistant_with(AlwaysRepliesTextLlm::new(), Arc::clone(&wire));

    run_session(assistant, Arc::clone(&wire));

    let sent = wire.lock().unwrap().sent.clone();
    let reply_index = sent
        .iter()
        .position(|message| matches!(message, ServerMessage::Reply { .. }))
        .expect("a Reply should have been sent");
    let (events_before, _) = only_reply_and_event_messages(&sent[..reply_index]);
    assert!(
        events_before > 0,
        "expected at least one Event before the Reply, got: {sent:?}"
    );
}

#[test]
fn an_llm_failure_sends_an_error_instead_of_a_reply() {
    let wire = Arc::new(Mutex::new(FakeWire::new(vec![ClientMessage::Input {
        text: "hola".to_string(),
    }])));
    let assistant = assistant_with(FailingLlm::new(), Arc::clone(&wire));

    run_session(assistant, Arc::clone(&wire));

    let sent = wire.lock().unwrap().sent.clone();
    assert!(matches!(sent.last(), Some(ServerMessage::Error { .. })));
}

#[test]
fn the_session_loop_ends_when_the_client_disconnects() {
    let wire = Arc::new(Mutex::new(FakeWire::new(vec![])));
    let assistant = assistant_with(AlwaysRepliesTextLlm::new(), Arc::clone(&wire));

    run_session(assistant, Arc::clone(&wire));

    // Only the greeting was sent — no turn ever ran.
    assert_eq!(wire.lock().unwrap().sent.len(), 1);
    assert!(matches!(
        wire.lock().unwrap().sent[0],
        ServerMessage::Greeting { .. }
    ));
}

#[test]
fn the_greeting_is_sent_before_anything_else() {
    let wire = Arc::new(Mutex::new(FakeWire::new(vec![ClientMessage::Input {
        text: "hola".to_string(),
    }])));
    let assistant = assistant_with(AlwaysRepliesTextLlm::new(), Arc::clone(&wire));

    run_session(assistant, Arc::clone(&wire));

    let sent = wire.lock().unwrap().sent.clone();
    assert!(matches!(sent.first(), Some(ServerMessage::Greeting { .. })));
}

#[test]
fn a_cancel_message_is_accepted_without_ending_the_session() {
    let wire = Arc::new(Mutex::new(FakeWire::new(vec![
        ClientMessage::Cancel,
        ClientMessage::Input {
            text: "hola".to_string(),
        },
    ])));
    let assistant = assistant_with(AlwaysRepliesTextLlm::new(), Arc::clone(&wire));

    run_session(assistant, Arc::clone(&wire));

    let sent = wire.lock().unwrap().sent.clone();
    assert!(matches!(sent.last(), Some(ServerMessage::Reply { .. })));
}
