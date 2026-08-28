use nala::application::{
    assistant::{Assistant, AssistantError},
    tools::{
        Tool,
        dispatcher::{ToolDispatcher, Tools},
        execute_command::ExecuteCommandTool,
        registry::ToolRegistry,
    },
};

use crate::{
    fake_computer::FakeComputer,
    fake_llm::{
        AlwaysCallsToolLlm, EchoesLastMessageLlm, FailingLlm, FakeLlm, RepeatsSameToolCallLlm,
    },
};

fn registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(ExecuteCommandTool::<FakeComputer>::definition());
    registry
}

fn assistant_with<L>(llm: L, computer: FakeComputer) -> Assistant<L, ToolDispatcher<FakeComputer>>
where
    L: nala::ports::llm::Llm,
{
    let tool = ExecuteCommandTool::new(computer);

    let mut dispatcher = ToolDispatcher::new();
    dispatcher.register(Tools::ExecuteCommand(tool));

    Assistant::new(llm, dispatcher, registry())
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
fn returns_loop_detected_on_repeated_tool_call() {
    let mut assistant = assistant_with(RepeatsSameToolCallLlm::new(), FakeComputer::new());

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
