use std::collections::VecDeque;

use agent_protocol::{ClientMessage, Event, LlmCallId, ServerMessage, TaskId};
use voice::client::{ClientError, NalaClient, Wire};

/// An in-memory `Wire`: `send` records every message it was asked to send,
/// `recv` pops from a scripted queue of server messages.
struct FakeWire {
    sent: Vec<ClientMessage>,
    incoming: VecDeque<ServerMessage>,
}

impl FakeWire {
    fn new(incoming: Vec<ServerMessage>) -> Self {
        Self {
            sent: Vec::new(),
            incoming: incoming.into(),
        }
    }
}

impl Wire for FakeWire {
    fn send(&mut self, message: &ClientMessage) -> Result<(), ClientError> {
        self.sent.push(message.clone());
        Ok(())
    }

    fn recv(&mut self) -> Result<Option<ServerMessage>, ClientError> {
        Ok(self.incoming.pop_front())
    }
}

fn task_id() -> TaskId {
    TaskId::new()
}

#[test]
fn recv_greeting_returns_the_greeting_text() {
    let wire = FakeWire::new(vec![ServerMessage::Event(Event::Greeting {
        text: "hola".to_string(),
    })]);
    let mut client = NalaClient::new(wire);

    let greeting = client.recv_greeting().unwrap();

    assert_eq!(greeting, "hola");
}

#[test]
fn recv_greeting_errors_on_anything_else() {
    let wire = FakeWire::new(vec![ServerMessage::Reply {
        text: "listo".to_string(),
    }]);
    let mut client = NalaClient::new(wire);

    let result = client.recv_greeting();

    assert!(matches!(result, Err(ClientError::Server(_))));
}

#[test]
fn recv_greeting_on_a_closed_connection_is_an_error_not_a_hang() {
    let wire = FakeWire::new(vec![]);
    let mut client = NalaClient::new(wire);

    let result = client.recv_greeting();

    assert!(matches!(result, Err(ClientError::ClosedWithoutReply)));
}

#[test]
fn a_stray_greeting_during_a_turn_is_forwarded_like_any_other_event() {
    let wire = FakeWire::new(vec![
        ServerMessage::Event(Event::Greeting {
            text: "hola".to_string(),
        }),
        ServerMessage::Reply {
            text: "listo".to_string(),
        },
    ]);
    let mut client = NalaClient::new(wire);

    let mut seen = Vec::new();
    let reply = client.send("hola", |event| seen.push(event)).unwrap();

    assert_eq!(seen.len(), 1);
    assert!(matches!(seen[0], Event::Greeting { .. }));
    assert_eq!(reply, "listo");
}

#[test]
fn sends_input_and_returns_the_reply_text() {
    let wire = FakeWire::new(vec![ServerMessage::Reply {
        text: "listo".to_string(),
    }]);
    let mut client = NalaClient::new(wire);

    let reply = client
        .send("hola", |_event| panic!("no event expected"))
        .unwrap();

    assert_eq!(reply, "listo");
}

#[test]
fn invokes_the_callback_for_every_event_before_the_reply() {
    let task_id = task_id();
    let wire = FakeWire::new(vec![
        ServerMessage::Event(Event::RequestStarted {
            task_id: task_id.clone(),
            prompt: "hola".to_string(),
            source: agent_protocol::RequestSource::Voice,
        }),
        ServerMessage::Event(Event::LlmStarted {
            llm_call_id: LlmCallId::new(&task_id, 1),
            task_id: task_id.clone(),
            call_index: 1,
            images: 0,
            provider: "ollama".to_string(),
            model: "gemma4:12b".to_string(),
        }),
        ServerMessage::Reply {
            text: "listo".to_string(),
        },
    ]);
    let mut client = NalaClient::new(wire);

    let mut seen = Vec::new();
    let reply = client.send("hola", |event| seen.push(event)).unwrap();

    assert_eq!(reply, "listo");
    assert_eq!(seen.len(), 2);
    assert!(matches!(seen[0], Event::RequestStarted { .. }));
    assert!(matches!(seen[1], Event::LlmStarted { .. }));
}

#[test]
fn a_server_error_message_is_returned_as_an_error_not_a_hang() {
    let wire = FakeWire::new(vec![ServerMessage::Error {
        message: "boom".to_string(),
    }]);
    let mut client = NalaClient::new(wire);

    let result = client.send("hola", |_event| {});

    assert!(matches!(result, Err(ClientError::Server(message)) if message == "boom"));
}

#[test]
fn the_connection_closing_without_a_reply_is_an_error_not_a_hang() {
    let wire = FakeWire::new(vec![]);
    let mut client = NalaClient::new(wire);

    let result = client.send("hola", |_event| {});

    assert!(matches!(result, Err(ClientError::ClosedWithoutReply)));
}
