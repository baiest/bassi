use nala::{
    adapters::events::console::ConsoleEventSink,
    application::{
        assistant::{Assistant, AssistantError},
        context_budget::ContextBudget,
        loop_limits::LoopLimits,
        tools::{
            Tool,
            computer_use::ComputerUseToolset,
            dispatcher::{ToolDispatcher, Tools},
            execute_command::ExecuteCommandTool,
            ping::PingTool,
            registry::ToolRegistry,
        },
    },
    ports::events::{Event, TurnState},
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
        MutatesThenChecksThenAnswersLlm, PlansThenExecutesLlm, RepeatsSameCallTwiceThenAnswersLlm,
        RepeatsSameToolCallLlm, RepliesWithLlm, RequestsTwoToolCallsAtOnceThenAnswersLlm,
        ResolvesInOneToolCallLlm, RetriesSameToolWithDifferentArgsLlm,
    },
    fake_mcp::FakeMcpClient,
    fake_speech::SpySpeech,
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
                    state: TurnState::Verifying
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
        .any(|event| matches!(event, Event::AnsweredUnverified));
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
                state: TurnState::Verifying
            }
        ) || matches!(event, Event::AnsweredUnverified)
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
    let toolset = ComputerUseToolset::connect(mcp, &["screenshot"]).unwrap();

    let mut dispatcher = ToolDispatcher::<FakeComputer, FakeMcpClient>::new();
    dispatcher.register(Tools::ExecuteCommand(ExecuteCommandTool::new(
        FakeComputer::new(),
    )));
    dispatcher.register(Tools::ComputerUse(toolset));

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

#[test]
fn speaks_the_final_answer() {
    let llm = AlwaysRepliesTextLlm::new();
    let computer = FakeComputer::new();
    let speech = SpySpeech::new();
    let spy = speech.clone();

    let mut assistant = assistant_with(llm, computer).with_speech(Box::new(speech));
    let result = assistant.process("hello");

    assert_eq!(result.unwrap(), "ok");
    assert_eq!(spy.spoken(), vec!["ok".to_string()]);
}

#[test]
fn speaks_a_multi_sentence_answer_in_a_single_call() {
    // Chunking is left to the streaming backend (server-side sentence
    // strategy) rather than split here into separate `say` calls, so a
    // multi-sentence answer still reaches `Speech::say` as one string.
    let llm = RepliesWithLlm::new("Hola. Como estas?");
    let computer = FakeComputer::new();
    let speech = SpySpeech::new();
    let spy = speech.clone();

    let mut assistant = assistant_with(llm, computer).with_speech(Box::new(speech));
    let result = assistant.process("hello");

    assert_eq!(result.unwrap(), "Hola. Como estas?");
    assert_eq!(spy.spoken(), vec!["Hola. Como estas?".to_string()]);
}

#[test]
fn speech_failure_does_not_break_the_turn() {
    let llm = AlwaysRepliesTextLlm::new();
    let computer = FakeComputer::new();
    let speech = SpySpeech::failing();

    let mut assistant = assistant_with(llm, computer).with_speech(Box::new(speech));
    let result = assistant.process("hola");

    assert_eq!(result.unwrap(), "ok");
}
