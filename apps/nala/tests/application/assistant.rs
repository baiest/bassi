use device_capabilities::Capability;
use device_capabilities::capabilities::execute_command::ExecuteCommandTool;
use nala::{
    adapters::events::console::ConsoleEventSink,
    adapters::memory::in_memory::InMemoryMemoryStore,
    adapters::metrics::csv_sink::CsvMetricsSink,
    application::{
        assistant::{Assistant, AssistantError},
        context_budget::ContextBudget,
        loop_limits::LoopLimits,
        tools::{
            Tool,
            dispatcher::{NoHttpFetcher, NoWallClock, ToolDispatcher, Tools},
            mcp_toolset::McpToolset,
            ping::PingTool,
            registry::ToolRegistry,
        },
    },
    ports::events::{Event, RequestSource, TurnState},
    ports::llm::Usage,
};

use crate::{
    fake_cancel::FakeCancelSignal,
    fake_clock::FakeClock,
    fake_computer::FakeComputer,
    fake_events::RecordingEventSink,
    fake_llm::{
        AlwaysAnswersEmptyLlm, AlwaysCallsToolLlm, AlwaysFailsRetryableLlm, AlwaysRepliesTextLlm,
        AnswersEmptyTwiceThenTextLlm, CallsScreenshotFiveTimesThenAnswersLlm,
        CallsScreenshotThenAnswersLlm, ChainsDistinctToolCallsThenAnswersLlm, EchoesLastMessageLlm,
        FailingLlm, FailsPlanningThenExecutesLlm, FailsTwiceThenSucceedsLlm,
        FailsWithInvalidResponseLlm, FakeLlm, HangsOnRealCallLlm, MutatesThenAnswersImmediatelyLlm,
        MutatesThenChecksThenAnswersLlm, PlansThenExecutesLlm, RecordsMessagesLlm,
        RepeatsSameCallTwiceThenAnswersLlm, RepeatsSameToolCallLlm, RepliesWithUsageLlm,
        RequestsTwoToolCallsAtOnceThenAnswersLlm, ResolvesInOneToolCallLlm,
        RetriesSameToolWithDifferentArgsLlm,
    },
    fake_mcp::FakeMcpClient,
};
use mcp::McpToolResult;

fn registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(ExecuteCommandTool::<FakeComputer>::definition().into());
    registry
}

fn assistant_with<L>(
    llm: L,
    computer: FakeComputer,
) -> Assistant<L, ToolDispatcher<FakeComputer>, ConsoleEventSink>
where
    L: nala::ports::llm::Llm + Send + 'static,
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
fn process_defaults_to_cli_as_the_request_source() {
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

    match &assistant.events().events[0] {
        Event::RequestStarted { source, .. } => assert_eq!(*source, RequestSource::Cli),
        other => panic!("expected RequestStarted, got {other:?}"),
    }
}

#[test]
fn process_from_carries_the_prompt_and_the_given_source() {
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

    assistant
        .process_from("open chrome", RequestSource::Android)
        .unwrap();

    match &assistant.events().events[0] {
        Event::RequestStarted { prompt, source, .. } => {
            assert_eq!(prompt, "open chrome");
            assert_eq!(*source, RequestSource::Android);
        }
        other => panic!("expected RequestStarted, got {other:?}"),
    }
}

#[test]
fn respond_to_runs_a_turn_tagged_as_the_autonomous_source() {
    use nala::ports::autonomous::AutonomousAgent;

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

    let reply = AutonomousAgent::respond_to(&mut assistant, "battery at 9%, act on it").unwrap();

    assert!(!reply.is_empty());
    match &assistant.events().events[0] {
        Event::RequestStarted { source, prompt, .. } => {
            assert_eq!(*source, RequestSource::Autonomous);
            assert_eq!(prompt, "battery at 9%, act on it");
        }
        other => panic!("expected RequestStarted, got {other:?}"),
    }
}

#[test]
fn stops_after_reaching_max_tool_calls() {
    // AlwaysCallsToolLlm targets an unregistered tool, so every call also
    // fails; disable the consecutive-failure limit so this test isolates
    // MAX_TOOL_CALLS instead of tripping TooManyConsecutiveToolFailures
    // first (that limit gets its own test below).
    let mut assistant =
        assistant_with(AlwaysCallsToolLlm::new(), FakeComputer::new()).with_limits(LoopLimits {
            max_consecutive_tool_failures: usize::MAX,
            ..LoopLimits::default()
        });

    let result = assistant.process("do something repeatedly");

    assert!(matches!(result, Err(AssistantError::ToolCallLimitExceeded)));
}

#[test]
fn returns_llm_error_when_generation_fails() {
    // FailingLlm's error is retryable; disabling retries here keeps this
    // test about "an LLM error surfaces as AssistantError::Llm", not about
    // retry behavior (which has its own tests below).
    let mut assistant =
        assistant_with(FailingLlm::new(), FakeComputer::new()).with_limits(LoopLimits {
            max_llm_retries: 0,
            ..LoopLimits::default()
        });

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
fn puts_the_invariant_computer_context_before_the_transcript() {
    // The computer context (username/dirs) never changes within a run, so
    // it belongs at the front of the prompt: everything after it grows by
    // appending, keeping the shared prefix stable across turns for a
    // backend that caches it (Ollama/llama.cpp). Putting it last, after the
    // transcript, would make that invariant content the part that keeps
    // moving instead.
    let llm = RecordsMessagesLlm::new();
    let received = llm.received.clone();

    let mut assistant = assistant_with(llm, FakeComputer::new());
    assistant.process("what time is it?").unwrap();

    let messages = received.lock().unwrap()[0].clone();
    let context_index = messages
        .iter()
        .position(|message| {
            message.role == "system" && message.content.contains("Computer context")
        })
        .expect("a system message with the computer context");
    let user_index = messages
        .iter()
        .position(|message| message.role == "user")
        .expect("the user's message");

    assert!(
        context_index < user_index,
        "expected computer context before the transcript, got: {messages:#?}"
    );
}

#[test]
fn injects_remembered_facts_as_a_system_message_before_the_user_message() {
    let llm = RecordsMessagesLlm::new();
    let received = llm.received.clone();

    let mut memory = InMemoryMemoryStore::new();
    nala::ports::memory::MemoryStore::remember(
        &mut memory,
        "nombre".to_string(),
        "Juan".to_string(),
    )
    .unwrap();

    let mut assistant = assistant_with(llm, FakeComputer::new()).with_memory(Box::new(memory));
    assistant.process("what time is it?").unwrap();

    let messages = received.lock().unwrap()[0].clone();
    let memory_index = messages
        .iter()
        .position(|message| message.role == "system" && message.content.contains("Juan"))
        .expect("a system message mentioning the remembered fact");
    let user_index = messages
        .iter()
        .position(|message| message.role == "user")
        .expect("the user's message");

    assert!(
        memory_index < user_index,
        "expected remembered facts before the transcript, got: {messages:#?}"
    );
}

#[test]
fn does_not_add_a_memory_message_when_nothing_is_remembered() {
    let llm = RecordsMessagesLlm::new();
    let received = llm.received.clone();

    let mut assistant =
        assistant_with(llm, FakeComputer::new()).with_memory(Box::new(InMemoryMemoryStore::new()));
    assistant.process("what time is it?").unwrap();

    let messages = received.lock().unwrap()[0].clone();
    assert!(
        !messages
            .iter()
            .any(|message| message.role == "system" && message.content.contains("Remembered")),
        "expected no memory system message when there are no facts, got: {messages:#?}"
    );
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

    assert!(matches!(result, Err(AssistantError::LoopDetected(_))));
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
fn prunes_old_history_once_it_exceeds_the_token_budget() {
    const TURNS: usize = 30;

    // A small enough budget that pruning has to kick in well before 30
    // turns' worth of "turn N" messages would fit, but large enough to
    // hold the system prompt plus a handful of them.
    let mut assistant = assistant_with(AlwaysRepliesTextLlm::new(), FakeComputer::new())
        .with_budget(ContextBudget {
            max_tokens: 500,
            output_reserve: 0,
            ..ContextBudget::default()
        });

    for turn in 0..TURNS {
        assistant
            .process(&format!("turn {turn}"))
            .expect("turn should succeed");
    }

    assert!(
        assistant.message_count() < TURNS * 2,
        "expected pruning to keep history well below {} messages, got {}",
        TURNS * 2,
        assistant.message_count()
    );
    let system_prompt = assistant
        .system_prompt()
        .expect("system prompt should survive pruning");
    assert!(system_prompt.starts_with("<role>\nYou are Nala, a general-purpose assistant."));
}

#[test]
fn emits_events_showing_images_reached_the_tool_result_and_the_next_llm_call() {
    let mcp = FakeMcpClient::new()
        .with_tool("screenshot", "Take a screenshot")
        .returning(McpToolResult {
            text: "here is the screen".to_string(),
            images: vec!["YmFzZTY0ZGF0YQ==".to_string()],
        });
    let toolset = McpToolset::connect(mcp, Some(&["screenshot"])).unwrap();

    let mut dispatcher =
        ToolDispatcher::<FakeComputer, NoWallClock, NoHttpFetcher, FakeMcpClient>::new();
    dispatcher.register(Tools::ExecuteCommand(ExecuteCommandTool::new(
        FakeComputer::new(),
    )));
    dispatcher.register(Tools::Mcp(vec![toolset]));

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
            Event::LlmStarted { images, .. } => Some(*images),
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

    let captured = messages_on_execute_call.lock().unwrap();
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
            Event::PlanCreated { plan, .. } => Some(plan.clone()),
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
            Event::StateChanged { state, .. } => Some(*state),
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
            // execute_command mutates; FakeLlm answers with text right
            // after it without a follow-up call, so the verification gate
            // fires once (Verifying) before the (still-unverified, but now
            // let through) answer.
            TurnState::Verifying,
            TurnState::Thinking,
            TurnState::Responding,
        ]
    );
}

#[test]
fn stops_after_too_many_consecutive_tool_failures() {
    // AlwaysCallsToolLlm targets an unregistered tool, so every call fails
    // with a distinct-arguments dispatcher error (never identical, so this
    // doesn't trip loop detection) — well under MAX_TOOL_CALLS too, so this
    // isolates the consecutive-failure limit.
    let mut assistant = assistant_with(AlwaysCallsToolLlm::new(), FakeComputer::new());

    let result = assistant.process("do something that keeps failing");

    assert!(matches!(
        result,
        Err(AssistantError::TooManyConsecutiveToolFailures(5))
    ));
}

#[test]
fn retries_a_retryable_llm_failure_and_recovers() {
    let clock = FakeClock::new();
    let sleeps = clock.sleeps();

    let mut assistant = assistant_with(FailsTwiceThenSucceedsLlm::new(), FakeComputer::new())
        .with_clock(Box::new(clock));

    let result = assistant.process("open chrome");

    assert_eq!(result.unwrap(), "recovered");
    assert_eq!(
        sleeps.borrow().len(),
        2,
        "expected one backoff sleep per retried failure"
    );
}

#[test]
fn gives_up_after_max_llm_retries_on_a_retryable_failure() {
    let clock = FakeClock::new();
    let sleeps = clock.sleeps();

    let mut assistant = assistant_with(AlwaysFailsRetryableLlm::new(), FakeComputer::new())
        .with_clock(Box::new(clock))
        .with_limits(LoopLimits {
            max_llm_retries: 2,
            ..LoopLimits::default()
        });

    let result = assistant.process("open chrome");

    assert!(matches!(result, Err(AssistantError::Llm(_))));
    assert_eq!(
        sleeps.borrow().len(),
        2,
        "expected exactly max_llm_retries sleeps"
    );
}

#[test]
fn does_not_retry_a_non_retryable_llm_failure() {
    let clock = FakeClock::new();
    let sleeps = clock.sleeps();

    let mut assistant = assistant_with(FailsWithInvalidResponseLlm::new(), FakeComputer::new())
        .with_clock(Box::new(clock));

    let result = assistant.process("open chrome");

    assert!(matches!(result, Err(AssistantError::Llm(_))));
    assert_eq!(
        sleeps.borrow().len(),
        0,
        "an invalid-response failure should not be retried"
    );
}

#[test]
fn cancelling_mid_call_abandons_a_hanging_llm_call_instead_of_waiting_for_it() {
    // Regression test: cancellation used to only be checked between LLM
    // calls, so Ctrl+C during a single (slow) call had no effect until that
    // call itself returned — which, against a real local model, can take
    // well over a minute. HangsOnRealCallLlm never returns from its "real"
    // call, so this only passes if `process` gives up on it directly.
    let cancel = FakeCancelSignal::new();
    let cancel_clone = cancel.clone();

    let mut assistant = assistant_with(HangsOnRealCallLlm::new(), FakeComputer::new())
        .with_cancel_signal(Box::new(cancel_clone));

    // Cancels shortly after the turn starts, from a separate thread, since
    // `process` below blocks the test thread until it returns.
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        cancel.cancel();
    });

    let start = std::time::Instant::now();
    let result = assistant.process("do something that never comes back");
    let elapsed = start.elapsed();

    assert!(matches!(result, Err(AssistantError::Cancelled)));
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "expected process() to abandon the hanging call promptly, took {elapsed:?}"
    );
}

#[test]
fn stops_when_cancelled_before_a_tool_call_runs() {
    let cancel = FakeCancelSignal::new();
    let cancel_clone = cancel.clone();

    // Cancels as soon as the first tool call would run; the fake LLM would
    // otherwise keep calling tools forever.
    let tool = ExecuteCommandTool::new(FakeComputer::new());
    let mut dispatcher = ToolDispatcher::<FakeComputer>::new();
    dispatcher.register(Tools::ExecuteCommand(tool));

    let mut assistant = Assistant::new(
        RepeatsSameToolCallLlm::new(),
        dispatcher,
        registry(),
        RecordingEventSink::new(),
    )
    .with_cancel_signal(Box::new(cancel_clone));

    cancel.cancel();

    let result = assistant.process("open chrome");

    assert!(matches!(result, Err(AssistantError::Cancelled)));
    let tool_starts = assistant
        .events()
        .events
        .iter()
        .filter(|event| matches!(event, Event::ToolStarted { .. }))
        .count();
    assert_eq!(
        tool_starts, 0,
        "no tool should run once cancellation was already requested"
    );
}

#[test]
fn gates_an_unverified_mutation_then_lets_it_through_once() {
    let events = RecordingEventSink::new();
    let mut assistant = Assistant::new(
        MutatesThenAnswersImmediatelyLlm::new(),
        {
            let tool = ExecuteCommandTool::new(FakeComputer::new());
            let mut dispatcher = ToolDispatcher::<FakeComputer>::new();
            dispatcher.register(Tools::ExecuteCommand(tool));
            dispatcher
        },
        registry(),
        events,
    );

    let result = assistant.process("create a folder");
    assert_eq!(
        result.unwrap(),
        "done",
        "the second, still-unverified answer should be let through rather than looping forever"
    );

    let verifying_states = assistant
        .events()
        .events
        .iter()
        .filter(|event| {
            matches!(
                event,
                Event::StateChanged {
                    state: TurnState::Verifying,
                    ..
                }
            )
        })
        .count();
    assert_eq!(
        verifying_states, 1,
        "expected exactly one verification-gate nudge"
    );

    let answered_unverified = assistant
        .events()
        .events
        .iter()
        .any(|event| matches!(event, Event::AnsweredUnverified { .. }));
    assert!(
        answered_unverified,
        "expected AnsweredUnverified once the gate let the second attempt through"
    );
}

#[test]
fn does_not_gate_when_a_mutation_was_followed_by_another_tool_call() {
    let events = RecordingEventSink::new();
    let mut assistant = Assistant::new(
        MutatesThenChecksThenAnswersLlm::new(),
        {
            let mut dispatcher = ToolDispatcher::<FakeComputer>::new();
            dispatcher.register(Tools::ExecuteCommand(ExecuteCommandTool::new(
                FakeComputer::new(),
            )));
            dispatcher.register(Tools::Ping(PingTool::new()));
            dispatcher
        },
        {
            let mut registry = registry();
            registry.register(PingTool::definition());
            registry
        },
        events,
    );

    let result = assistant.process("create a folder and check it");
    assert_eq!(result.unwrap(), "done");

    let gated = assistant.events().events.iter().any(|event| {
        matches!(
            event,
            Event::StateChanged {
                state: TurnState::Verifying,
                ..
            }
        ) || matches!(event, Event::AnsweredUnverified { .. })
    });
    assert!(
        !gated,
        "a tool call after the mutation should count as checking it, no gate expected"
    );
}

#[test]
fn evicts_old_images_once_the_turn_exceeds_its_token_budget() {
    let mcp = FakeMcpClient::new()
        .with_tool("screenshot", "Take a screenshot")
        .returning(McpToolResult {
            text: "here is the screen".to_string(),
            images: vec!["YmFzZTY0ZGF0YQ==".to_string()],
        });
    let toolset = McpToolset::connect(mcp, Some(&["screenshot"])).unwrap();

    let mut dispatcher =
        ToolDispatcher::<FakeComputer, NoWallClock, NoHttpFetcher, FakeMcpClient>::new();
    dispatcher.register(Tools::ExecuteCommand(ExecuteCommandTool::new(
        FakeComputer::new(),
    )));
    dispatcher.register(Tools::Mcp(vec![toolset]));

    let events = RecordingEventSink::new();
    let mut assistant = Assistant::new(
        CallsScreenshotFiveTimesThenAnswersLlm::new(),
        dispatcher,
        registry(),
        events,
    )
    .with_budget(ContextBudget {
        // 5 screenshots would cost far more than this; only the 2 most
        // recent should survive once pressure kicks in.
        max_tokens: 2000,
        output_reserve: 0,
        keep_recent_images: 2,
        ..ContextBudget::default()
    });

    let result = assistant.process("take five screenshots");
    assert_eq!(result.unwrap(), "done");

    let dropped_image_events: usize = assistant
        .events()
        .events
        .iter()
        .filter_map(|event| match event {
            Event::BudgetPressure {
                step: nala::ports::events::BudgetStep::DroppedImages { count },
                ..
            } => Some(count),
            _ => None,
        })
        .sum();
    assert!(
        dropped_image_events > 0,
        "expected at least one image to have been evicted for budget pressure"
    );
}

#[test]
fn emits_tokens_used_after_a_completed_llm_call() {
    let stub_llm = FakeLlm::new();

    let events = RecordingEventSink::new();
    let mut assistant = Assistant::new(
        stub_llm,
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

    let tokens_used = assistant
        .events()
        .events
        .iter()
        .any(|event| matches!(event, Event::TokensUsed { .. }));
    assert!(
        tokens_used,
        "expected at least one TokensUsed event after a completed call"
    );
}

#[test]
fn emits_tokens_used_for_the_planning_call_too() {
    // FakeLlm's planning branch (tools.is_empty()), its tool-call round, and
    // its final-text round each hit `generate` — plus one more: the
    // execute_command tool mutates, so the verification gate holds the
    // first text answer back and forces one extra round before letting it
    // through. Four LLM calls in total for "open chrome". Every one of
    // them should produce a TokensUsed event, not just the ones the main
    // loop makes directly.
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

    let tokens_used_count = assistant
        .events()
        .events
        .iter()
        .filter(|event| matches!(event, Event::TokensUsed { .. }))
        .count();
    assert_eq!(
        tokens_used_count, 4,
        "expected the planning call to also produce a TokensUsed event"
    );
}

#[test]
fn instruments_the_summarization_call_made_by_compaction() {
    // Same setup as `compacts_old_turns_into_a_summary_once_the_budget_is_exceeded`,
    // but this asserts the summarization call itself (made from `compact`,
    // via `call_llm` directly today) is instrumented like any other LLM
    // call: planning + the tool-call round + the compaction summary + the
    // final answer round is 4 calls total.
    let events = RecordingEventSink::new();
    let mut assistant = Assistant::new(
        RequestsTwoToolCallsAtOnceThenAnswersLlm::new(),
        {
            let tool = ExecuteCommandTool::new(FakeComputer::new());
            let mut dispatcher = ToolDispatcher::<FakeComputer>::new();
            dispatcher.register(Tools::ExecuteCommand(tool));
            dispatcher
        },
        registry(),
        events,
    )
    .with_budget(ContextBudget {
        max_tokens: 5,
        output_reserve: 0,
        keep_recent_uncompacted: 0,
        ..ContextBudget::default()
    });

    let result = assistant.process("run two commands");
    assert_eq!(result.unwrap(), "done");

    let llm_completed_count = assistant
        .events()
        .events
        .iter()
        .filter(|event| matches!(event, Event::LlmCompleted { .. }))
        .count();
    assert!(
        llm_completed_count >= 4,
        "expected the compaction summary call to also emit LlmCompleted, got {llm_completed_count}"
    );
}

#[test]
fn a_failed_llm_call_emits_llm_failed_not_request_failed() {
    let events = RecordingEventSink::new();
    let mut assistant = Assistant::new(
        FailingLlm::new(),
        {
            let tool = ExecuteCommandTool::new(FakeComputer::new());
            let mut dispatcher = ToolDispatcher::<FakeComputer>::new();
            dispatcher.register(Tools::ExecuteCommand(tool));
            dispatcher
        },
        registry(),
        events,
    )
    .with_limits(LoopLimits {
        max_llm_retries: 0,
        ..LoopLimits::default()
    });

    let result = assistant.process("open chrome");
    assert!(matches!(result, Err(AssistantError::Llm(_))));

    let llm_failed = assistant
        .events()
        .events
        .iter()
        .any(|event| matches!(event, Event::LlmFailed { .. }));
    assert!(llm_failed, "expected a failed LLM call to emit LlmFailed");

    let request_failed = assistant
        .events()
        .events
        .iter()
        .any(|event| matches!(event, Event::RequestFailed { .. }));
    assert!(
        !request_failed,
        "a bare LLM-call failure shouldn't also emit RequestFailed \
         (that's reserved for task-level abort/cancellation)"
    );
}

#[test]
fn every_event_in_a_task_shares_the_same_task_id() {
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

    fn task_id_of(event: &Event) -> &nala::ports::events::TaskId {
        match event {
            Event::RequestStarted { task_id, .. }
            | Event::StateChanged { task_id, .. }
            | Event::RequestCompleted { task_id, .. }
            | Event::RequestFailed { task_id, .. }
            | Event::PlanCreated { task_id, .. }
            | Event::LlmStarted { task_id, .. }
            | Event::LlmCompleted { task_id, .. }
            | Event::LlmFailed { task_id, .. }
            | Event::ToolStarted { task_id, .. }
            | Event::ToolCompleted { task_id, .. }
            | Event::Retrying { task_id, .. }
            | Event::Cancelled { task_id }
            | Event::TokensUsed { task_id, .. }
            | Event::BudgetPressure { task_id, .. }
            | Event::TranscriptCompacted { task_id, .. }
            | Event::AnsweredUnverified { task_id } => task_id,
            Event::Greeting { .. } => {
                panic!("Assistant::process never emits Greeting")
            }
            Event::AutonomousEventReceived { .. }
            | Event::AutonomousEventIgnored { .. }
            | Event::AutonomousEventDelegated { .. }
            | Event::AutonomousEventCompleted { .. }
            | Event::AutonomousEventFailed { .. } => {
                panic!("Assistant::process never emits autonomous events")
            }
        }
    }

    let events = &assistant.events().events;
    assert!(!events.is_empty());
    let first_task_id = task_id_of(&events[0]);
    for event in events {
        assert_eq!(
            task_id_of(event),
            first_task_id,
            "every event in one process() call should share its task_id"
        );
    }
}

#[test]
fn two_process_calls_produce_distinct_task_ids() {
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
    let first_task_id = match &assistant.events().events[0] {
        Event::RequestStarted { task_id, .. } => task_id.clone(),
        other => panic!("expected RequestStarted, got {other:?}"),
    };

    assistant.process("open chrome").unwrap();
    let events = &assistant.events().events;
    let second_task_id = match events
        .iter()
        .rev()
        .find(|event| matches!(event, Event::RequestStarted { .. }))
        .unwrap()
    {
        Event::RequestStarted { task_id, .. } => task_id.clone(),
        _ => unreachable!(),
    };

    assert_ne!(
        first_task_id, second_task_id,
        "each process() call should get its own task_id"
    );
}

#[test]
fn llm_call_index_is_sequential_across_the_whole_task_including_planning() {
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

    let call_indices: Vec<u32> = assistant
        .events()
        .events
        .iter()
        .filter_map(|event| match event {
            Event::LlmStarted { call_index, .. } => Some(*call_index),
            _ => None,
        })
        .collect();

    let expected: Vec<u32> = (1..=call_indices.len() as u32).collect();
    assert_eq!(
        call_indices, expected,
        "call_index should run 1..N without gaps or repeats, planning included"
    );
}

#[test]
fn tool_events_carry_the_task_id_and_a_sequential_tool_call_index() {
    let events = RecordingEventSink::new();
    let mut assistant = Assistant::new(
        ChainsDistinctToolCallsThenAnswersLlm::new(),
        {
            let tool = ExecuteCommandTool::new(FakeComputer::new());
            let mut dispatcher = ToolDispatcher::<FakeComputer>::new();
            dispatcher.register(Tools::ExecuteCommand(tool));
            dispatcher
        },
        registry(),
        events,
    );

    let result = assistant.process("do a multi-step task");
    assert_eq!(result.unwrap(), "done");

    let started_task_id = match &assistant.events().events[0] {
        Event::RequestStarted { task_id, .. } => task_id.clone(),
        other => panic!("expected RequestStarted, got {other:?}"),
    };

    let tool_indices: Vec<u32> = assistant
        .events()
        .events
        .iter()
        .filter_map(|event| match event {
            Event::ToolStarted {
                task_id,
                tool_call_index,
                ..
            } => {
                assert_eq!(task_id, &started_task_id);
                Some(*tool_call_index)
            }
            _ => None,
        })
        .collect();

    assert_eq!(tool_indices, vec![1, 2]);
}

#[test]
fn compacts_old_turns_into_a_summary_once_the_budget_is_exceeded() {
    // A very small budget with generous keep_recent_uncompacted forces
    // fit_to_budget past the deterministic steps (no images, nothing long
    // enough to truncate) straight to compaction.
    let events = RecordingEventSink::new();
    let mut assistant = Assistant::new(
        RequestsTwoToolCallsAtOnceThenAnswersLlm::new(),
        {
            let tool = ExecuteCommandTool::new(FakeComputer::new());
            let mut dispatcher = ToolDispatcher::<FakeComputer>::new();
            dispatcher.register(Tools::ExecuteCommand(tool));
            dispatcher
        },
        registry(),
        events,
    )
    .with_budget(ContextBudget {
        max_tokens: 5,
        output_reserve: 0,
        keep_recent_uncompacted: 0,
        ..ContextBudget::default()
    });

    let result = assistant.process("run two commands");
    assert_eq!(result.unwrap(), "done");

    let compacted = assistant
        .events()
        .events
        .iter()
        .any(|event| matches!(event, Event::TranscriptCompacted { .. }));
    assert!(
        compacted,
        "expected compaction to fire once deterministic eviction wasn't enough"
    );
}

#[test]
fn continues_without_a_plan_when_the_planning_call_fails() {
    let mut assistant = assistant_with(FailsPlanningThenExecutesLlm::new(), FakeComputer::new());

    let result = assistant.process("pon musica en spotify");

    assert_eq!(result.unwrap(), "done");
}

/// A fresh, empty directory per test — no external tempfile crate needed,
/// `CsvMetricsSink`'s own write path creates the directory if missing.
fn metrics_temp_dir() -> std::path::PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "nala_assistant_metrics_test_{}_{n}",
        std::process::id()
    ))
}

fn read_csv(path: &std::path::Path) -> Vec<Vec<String>> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .from_path(path)
        .unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()));
    reader
        .records()
        .map(|record| {
            record
                .unwrap()
                .iter()
                .map(|field| field.to_string())
                .collect()
        })
        .collect()
}

#[test]
fn csv_metrics_sink_records_a_real_task_end_to_end() {
    // Exercises the real Assistant/agent_loop wiring through
    // `CsvMetricsSink` (not hand-built events, unlike the sink's own unit
    // tests in tests/adapters/metrics/csv_sink.rs), to prove it correctly
    // consumes what the loop actually emits.
    let dir = metrics_temp_dir();
    let usage = Usage {
        prompt_tokens: Some(42),
        completion_tokens: Some(17),
    };
    let events = CsvMetricsSink::new(RecordingEventSink::new(), Some(dir.clone()));

    let tool = ExecuteCommandTool::new(FakeComputer::new());
    let mut dispatcher = ToolDispatcher::<FakeComputer>::new();
    dispatcher.register(Tools::ExecuteCommand(tool));

    let mut assistant = Assistant::new(
        RepliesWithUsageLlm::new("done", usage),
        dispatcher,
        registry(),
        events,
    );

    let result = assistant.process("say something");
    assert_eq!(result.unwrap(), "done");

    let llm_rows = read_csv(&dir.join("llm_calls.csv"));
    let data_rows = &llm_rows[1..];
    assert!(!data_rows.is_empty());
    let total_input: u32 = data_rows
        .iter()
        .map(|row| row[6].parse::<u32>().unwrap())
        .sum();
    let total_output: u32 = data_rows
        .iter()
        .map(|row| row[7].parse::<u32>().unwrap())
        .sum();
    assert_eq!(total_input, 42 * data_rows.len() as u32);
    assert_eq!(total_output, 17 * data_rows.len() as u32);

    let task_rows = read_csv(&dir.join("tasks.csv"));
    assert_eq!(task_rows.len(), 2);
    let row = &task_rows[1];
    assert_eq!(row[4].parse::<u32>().unwrap(), data_rows.len() as u32);
    assert_eq!(row[5].parse::<u32>().unwrap(), total_input);
    assert_eq!(row[6].parse::<u32>().unwrap(), total_output);
    assert_eq!(row[10], "ok");
}
