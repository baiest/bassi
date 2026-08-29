use nala::{
    adapters::events::console::ConsoleEventSink,
    application::{
        assistant::{Assistant, AssistantError, MAX_HISTORY_MESSAGES},
        tools::{
            Tool,
            computer_use::ComputerUseToolset,
            dispatcher::{ToolDispatcher, Tools},
            execute_command::ExecuteCommandTool,
            registry::ToolRegistry,
        },
    },
    ports::events::{Event, TurnState},
};

use crate::{
    fake_computer::FakeComputer,
    fake_events::RecordingEventSink,
    fake_llm::{
        AlwaysAnswersEmptyLlm, AlwaysCallsToolLlm, AlwaysRepliesTextLlm,
        AnswersEmptyTwiceThenTextLlm, CallsScreenshotThenAnswersLlm,
        ChainsDistinctToolCallsThenAnswersLlm, EchoesLastMessageLlm, FailingLlm,
        FailsPlanningThenExecutesLlm, FakeLlm, PlansThenExecutesLlm,
        RepeatsSameCallTwiceThenAnswersLlm, RepeatsSameToolCallLlm,
        RequestsTwoToolCallsAtOnceThenAnswersLlm, ResolvesInOneToolCallLlm,
        RetriesSameToolWithDifferentArgsLlm,
    },
    fake_mcp::FakeMcpClient,
};
use nala::ports::mcp::McpToolResult;

fn registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(ExecuteCommandTool::<FakeComputer>::definition());
    registry
}

fn assistant_with<L>(
    llm: L,
    computer: FakeComputer,
) -> Assistant<L, ToolDispatcher<FakeComputer>, ConsoleEventSink>
where
    L: nala::ports::llm::Llm,
{
    let tool = ExecuteCommandTool::new(computer);

    let mut dispatcher = ToolDispatcher::new();
    dispatcher.register(Tools::ExecuteCommand(tool));

    let events = ConsoleEventSink;

    Assistant::new(llm, dispatcher, registry(), events)
}

#[test]
fn executes_tool_requested_by_llm() {
    let mut assistant = assistant_with(FakeLlm::new(), FakeComputer::new());

    let result = assistant.process("open chrome");

    assert_eq!(result.unwrap(), "chrome opened");
}

#[test]
fn stops_after_reaching_max_tool_calls() {
    let mut assistant = assistant_with(AlwaysCallsToolLlm::new(), FakeComputer::new());

    let result = assistant.process("do something repeatedly");

    assert!(matches!(result, Err(AssistantError::ToolCallLimitExceeded)));
}

#[test]
fn returns_llm_error_when_generation_fails() {
    let mut assistant = assistant_with(FailingLlm::new(), FakeComputer::new());

    let result = assistant.process("open chrome");

    assert!(matches!(result, Err(AssistantError::Llm(_))));
}

#[test]
fn returns_tool_error_when_context_fails() {
    let mut computer = FakeComputer::new();
    computer.should_fail_context = true;

    let mut assistant = assistant_with(FakeLlm::new(), computer);

    let result = assistant.process("open chrome");

    assert!(matches!(result, Err(AssistantError::Tool(_))));
}

#[test]
fn chains_down_to_the_underlying_tool_error() {
    use std::error::Error as StdError;

    let mut computer = FakeComputer::new();
    computer.should_fail_context = true;

    let mut assistant = assistant_with(FakeLlm::new(), computer);

    let error = assistant.process("open chrome").unwrap_err();

    // AssistantError -> ToolDispatcherError -> the tool's own error, each
    // level readable through Display and reachable through source().
    assert!(!error.to_string().is_empty());
    let source = error.source().expect("AssistantError should have a source");
    assert!(source.source().is_some());
}

#[test]
fn feeds_tool_error_back_to_llm() {
    let mut computer = FakeComputer::new();
    computer.should_fail = true;

    let mut assistant = assistant_with(EchoesLastMessageLlm::new(), computer);

    let result = assistant.process("open chrome");

    let text = result.unwrap();
    assert!(
        text.starts_with("ERROR:"),
        "expected an error message, got {text:?}"
    );
}

#[test]
fn resolves_simple_request_in_a_single_tool_call() {
    let mut assistant = assistant_with(ResolvesInOneToolCallLlm::new(), FakeComputer::new());

    let result = assistant.process("what time is it?");

    assert_eq!(result.unwrap(), "it's 10:00 AM");
}

#[test]
fn stops_after_reaching_max_tool_calls_when_the_llm_keeps_varying_the_arguments() {
    // A tool succeeding doesn't end the turn by itself: the model has to
    // recognize the task is done and answer with text. If it keeps calling
    // the same tool with different arguments forever, the turn still has to
    // end via the tool-call limit, not a stale "already succeeded" shortcut.
    let mut assistant = assistant_with(
        RetriesSameToolWithDifferentArgsLlm::new(),
        FakeComputer::new(),
    );

    let result = assistant.process("what time is it?");

    assert!(matches!(result, Err(AssistantError::ToolCallLimitExceeded)));
}

#[test]
fn chains_distinct_tool_calls_before_answering() {
    // A real computer-use flow looks like screenshot -> click -> screenshot:
    // several distinct calls to the same tool name, none of them identical,
    // followed by a text answer. None of this should be mistaken for a loop.
    let mut assistant = assistant_with(
        ChainsDistinctToolCallsThenAnswersLlm::new(),
        FakeComputer::new(),
    );

    let result = assistant.process("do a multi-step task");

    assert_eq!(result.unwrap(), "done");
}

#[test]
fn tolerates_the_same_tool_call_repeated_below_the_loop_threshold() {
    let mut assistant = assistant_with(
        RepeatsSameCallTwiceThenAnswersLlm::new(),
        FakeComputer::new(),
    );

    let result = assistant.process("what time is it?");

    assert_eq!(result.unwrap(), "done");
}

#[test]
fn returns_loop_detected_on_repeated_failing_tool_call() {
    let mut computer = FakeComputer::new();
    computer.should_fail = true;

    let mut assistant = assistant_with(RepeatsSameToolCallLlm::new(), computer);

    let result = assistant.process("open chrome");

    assert!(matches!(result, Err(AssistantError::LoopDetected)));
}

#[test]
fn keeps_conversation_history_across_process_calls() {
    let mut assistant = assistant_with(FakeLlm::new(), FakeComputer::new());

    let first = assistant.process("open chrome");
    assert!(first.is_ok());

    // The fake LLM ignores its input and always returns text on its second
    // call within a turn, but a fresh turn restarts its own call counter, so
    // a second `process` call still resolves without needing new tool calls.
    let second = assistant.process("thanks");
    assert!(second.is_ok());
}

#[test]
fn keeps_only_the_last_messages_in_history() {
    let mut assistant = assistant_with(AlwaysRepliesTextLlm::new(), FakeComputer::new());

    for turn in 0..(MAX_HISTORY_MESSAGES + 10) {
        assistant
            .process(&format!("turn {turn}"))
            .expect("turn should succeed");
    }

    assert!(assistant.message_count() <= MAX_HISTORY_MESSAGES);
    let system_prompt = assistant
        .system_prompt()
        .expect("system prompt should survive pruning");
    assert!(system_prompt.starts_with("<role>\nYou are Nala, a computer assistant."));
}

#[test]
fn emits_events_showing_images_reached_the_tool_result_and_the_next_llm_call() {
    let mcp = FakeMcpClient::new()
        .with_tool("screenshot", "Take a screenshot")
        .returning(McpToolResult {
            text: "here is the screen".to_string(),
            images: vec!["YmFzZTY0ZGF0YQ==".to_string()],
        });
    let toolset = ComputerUseToolset::connect(mcp, &["screenshot"]).unwrap();

    let mut dispatcher = ToolDispatcher::<FakeComputer, FakeMcpClient>::new();
    dispatcher.register(Tools::ExecuteCommand(ExecuteCommandTool::new(
        FakeComputer::new(),
    )));
    dispatcher.register(Tools::ComputerUse(toolset));

    let events = RecordingEventSink::new();
    let mut assistant = Assistant::new(
        CallsScreenshotThenAnswersLlm::new(),
        dispatcher,
        registry(),
        events,
    );

    let result = assistant.process("take a screenshot");
    assert_eq!(result.unwrap(), "done");

    let events = assistant.events().events.as_slice();

    let tool_completed_images = events.iter().find_map(|event| match event {
        Event::ToolCompleted { images, .. } => Some(*images),
        _ => None,
    });
    assert_eq!(
        tool_completed_images,
        Some(1),
        "expected the screenshot's image to be reported on ToolCompleted"
    );

    let llm_started_image_counts: Vec<usize> = events
        .iter()
        .filter_map(|event| match event {
            Event::LlmStarted { images } => Some(*images),
            _ => None,
        })
        .collect();
    assert_eq!(
        llm_started_image_counts,
        vec![0, 0, 1],
        "the planning call, then the screenshot call, then the call after \
         it should report the screenshot's image"
    );
}

#[test]
fn generates_a_plan_before_executing_and_keeps_it_in_context() {
    let llm = PlansThenExecutesLlm::new("1. abre spotify\n2. dale play a la cancion");
    let messages_on_execute_call = llm.messages_on_execute_call.clone();

    let mut assistant = assistant_with(llm, FakeComputer::new());

    let result = assistant.process("pon musica en spotify");

    assert_eq!(result.unwrap(), "done");

    let captured = messages_on_execute_call.borrow();
    let captured = captured
        .as_ref()
        .expect("the execute-step call should have happened");

    assert!(
        captured
            .iter()
            .any(|message| message.content.contains("abre spotify")),
        "expected the generated plan to be present in context for the execute call"
    );
}

#[test]
fn emits_plan_created_with_the_generated_plan() {
    let llm = PlansThenExecutesLlm::new("1. abre spotify\n2. dale play a la cancion");
    let tool = ExecuteCommandTool::new(FakeComputer::new());

    let mut dispatcher = ToolDispatcher::<FakeComputer>::new();
    dispatcher.register(Tools::ExecuteCommand(tool));

    let events = RecordingEventSink::new();
    let mut assistant = Assistant::new(llm, dispatcher, registry(), events);

    assistant.process("pon musica en spotify").unwrap();

    let plan = assistant
        .events()
        .events
        .iter()
        .find_map(|event| match event {
            Event::PlanCreated { plan } => Some(plan.clone()),
            _ => None,
        });

    assert_eq!(
        plan,
        Some("1. abre spotify\n2. dale play a la cancion".to_string())
    );
}

#[test]
fn executes_all_tool_calls_requested_in_a_single_llm_response() {
    let events = RecordingEventSink::new();
    let tool = ExecuteCommandTool::new(FakeComputer::new());
    let mut dispatcher = ToolDispatcher::<FakeComputer>::new();
    dispatcher.register(Tools::ExecuteCommand(tool));

    let mut assistant = Assistant::new(
        RequestsTwoToolCallsAtOnceThenAnswersLlm::new(),
        dispatcher,
        registry(),
        events,
    );

    let result = assistant.process("run two commands");
    assert_eq!(result.unwrap(), "done");

    let tool_starts = assistant
        .events()
        .events
        .iter()
        .filter(|event| matches!(event, Event::ToolStarted { .. }))
        .count();
    assert_eq!(
        tool_starts, 2,
        "both tool calls from the single LLM response should have run"
    );
}

#[test]
fn nudges_the_model_and_continues_on_an_empty_response() {
    let mut assistant = assistant_with(AnswersEmptyTwiceThenTextLlm::new(), FakeComputer::new());

    let result = assistant.process("say something");

    assert_eq!(result.unwrap(), "done");
}

#[test]
fn gives_up_when_the_model_keeps_answering_empty() {
    let mut assistant = assistant_with(AlwaysAnswersEmptyLlm::new(), FakeComputer::new());

    let result = assistant.process("say something");

    assert!(matches!(result, Err(AssistantError::EmptyResponse)));
}

#[test]
fn emits_turn_states_in_order() {
    let events = RecordingEventSink::new();
    let mut assistant = Assistant::new(
        FakeLlm::new(),
        {
            let tool = ExecuteCommandTool::new(FakeComputer::new());
            let mut dispatcher = ToolDispatcher::<FakeComputer>::new();
            dispatcher.register(Tools::ExecuteCommand(tool));
            dispatcher
        },
        registry(),
        events,
    );

    assistant.process("open chrome").unwrap();

    let states: Vec<TurnState> = assistant
        .events()
        .events
        .iter()
        .filter_map(|event| match event {
            Event::StateChanged { state } => Some(*state),
            _ => None,
        })
        .collect();

    assert_eq!(
        states,
        vec![
            TurnState::Receiving,
            TurnState::Planning,
            TurnState::Thinking,
            TurnState::Executing,
            TurnState::Thinking,
            TurnState::Responding,
        ]
    );
}

#[test]
fn continues_without_a_plan_when_the_planning_call_fails() {
    let mut assistant = assistant_with(FailsPlanningThenExecutesLlm::new(), FakeComputer::new());

    let result = assistant.process("pon musica en spotify");

    assert_eq!(result.unwrap(), "done");
}
