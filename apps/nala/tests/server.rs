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
use nala::server::{Wire, WsEventSink, build_events, run_session};

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
        source: agent_protocol::RequestSource::Cli,
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
fn the_client_supplied_source_reaches_the_request_started_event() {
    let wire = Arc::new(Mutex::new(FakeWire::new(vec![ClientMessage::Input {
        text: "hola".to_string(),
        source: agent_protocol::RequestSource::Android,
    }])));
    let assistant = assistant_with(AlwaysRepliesTextLlm::new(), Arc::clone(&wire));

    run_session(assistant, Arc::clone(&wire));

    let sent = wire.lock().unwrap().sent.clone();
    let started = sent.iter().find_map(|message| match message {
        ServerMessage::Event(agent_protocol::Event::RequestStarted { source, .. }) => Some(*source),
        _ => None,
    });
    assert_eq!(started, Some(agent_protocol::RequestSource::Android));
}

#[test]
fn progress_events_are_sent_before_the_final_reply() {
    let wire = Arc::new(Mutex::new(FakeWire::new(vec![ClientMessage::Input {
        text: "hola".to_string(),
        source: agent_protocol::RequestSource::Cli,
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
        source: agent_protocol::RequestSource::Cli,
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

    assert!(wire.lock().unwrap().sent.is_empty());
}

/// A fresh, unique temp directory per test, mirroring the helper in
/// `tests/adapters/metrics/csv_sink.rs`.
fn temp_dir() -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "nala_server_metrics_test_{}_{n}_{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

/// Regression test for the bug where `nala --serve` never wired
/// `CsvMetricsSink`, so every real request (overlay, Android, voice) left
/// no trace in `data/metrics/*.csv` — only the local CLI REPL did. This
/// checks the same `build_events` helper `handle_connection` uses.
#[test]
fn a_served_session_writes_a_task_row_to_the_metrics_csv() {
    let dir = temp_dir();
    let wire = Arc::new(Mutex::new(FakeWire::new(vec![ClientMessage::Input {
        text: "hola".to_string(),
        source: agent_protocol::RequestSource::Cli,
    }])));
    let events = build_events(Arc::clone(&wire), Some(dir.clone()));

    let mut registry = ToolRegistry::new();
    registry.register(ExecuteCommandTool::<FakeComputer>::definition().into());
    let mut dispatcher: ToolDispatcher<FakeComputer> = ToolDispatcher::new();
    dispatcher.register(Tools::ExecuteCommand(ExecuteCommandTool::new(
        FakeComputer::new(),
    )));
    let assistant = Assistant::new(AlwaysRepliesTextLlm::new(), dispatcher, registry, events);

    run_session(assistant, Arc::clone(&wire));

    let tasks_csv = std::fs::read_to_string(dir.join("tasks.csv"))
        .unwrap_or_else(|error| panic!("expected tasks.csv to be written: {error}"));
    assert_eq!(
        tasks_csv.lines().count(),
        2,
        "expected a header plus one task row, got:\n{tasks_csv}"
    );
}

#[test]
fn a_cancel_message_is_accepted_without_ending_the_session() {
    let wire = Arc::new(Mutex::new(FakeWire::new(vec![
        ClientMessage::Cancel,
        ClientMessage::Input {
            text: "hola".to_string(),
            source: agent_protocol::RequestSource::Cli,
        },
    ])));
    let assistant = assistant_with(AlwaysRepliesTextLlm::new(), Arc::clone(&wire));

    run_session(assistant, Arc::clone(&wire));

    let sent = wire.lock().unwrap().sent.clone();
    assert!(matches!(sent.last(), Some(ServerMessage::Reply { .. })));
}
