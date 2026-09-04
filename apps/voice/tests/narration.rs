use std::time::Duration;

use agent_protocol::{Event, LlmCallId, TaskId, TurnState};
use voice::narration::TemplateNarrator;
use voice::narrator::Narrator;

fn task_id() -> TaskId {
    TaskId::new()
}

fn llm_call_id() -> LlmCallId {
    LlmCallId::new(&task_id(), 1)
}

#[test]
fn narrates_a_known_tool_starting() {
    let mut narrator = TemplateNarrator::new();

    let phrase = narrator.narrate(&Event::ToolStarted {
        task_id: task_id(),
        tool_call_index: 1,
        name: "execute_command".to_string(),
        arguments: "{}".to_string(),
    });

    assert_eq!(phrase, Some("Voy a ejecutar un comando.".to_string()));
}

#[test]
fn narrates_an_unrecognized_tool_with_a_generic_phrase() {
    let mut narrator = TemplateNarrator::new();

    let phrase = narrator.narrate(&Event::ToolStarted {
        task_id: task_id(),
        tool_call_index: 1,
        name: "search_the_web".to_string(),
        arguments: "{}".to_string(),
    });

    assert!(phrase.is_some());
}

#[test]
fn rotates_phrases_for_the_same_state_across_a_turn() {
    let mut narrator = TemplateNarrator::new();

    let first = narrator
        .narrate(&Event::StateChanged {
            task_id: task_id(),
            state: TurnState::Thinking,
        })
        .expect("first Thinking should narrate");

    // Something else happens in between, so the repeat-suppression rule
    // doesn't swallow the second Thinking. `Executing` itself doesn't
    // narrate (see `narration.rs`), so a real intervening event is needed
    // here — a tool starting is what actually separates two `Thinking`
    // states in a real turn.
    narrator.narrate(&Event::ToolStarted {
        task_id: task_id(),
        tool_call_index: 1,
        name: "execute_command".to_string(),
        arguments: "{}".to_string(),
    });

    let second = narrator
        .narrate(&Event::StateChanged {
            task_id: task_id(),
            state: TurnState::Thinking,
        })
        .expect("second Thinking should still narrate");

    assert_ne!(first, second);
}

#[test]
fn suppresses_the_same_state_repeated_back_to_back() {
    let mut narrator = TemplateNarrator::new();

    narrator
        .narrate(&Event::StateChanged {
            task_id: task_id(),
            state: TurnState::Thinking,
        })
        .expect("first Thinking should narrate");

    let second = narrator.narrate(&Event::StateChanged {
        task_id: task_id(),
        state: TurnState::Thinking,
    });

    assert_eq!(second, None);
}

#[test]
fn does_not_narrate_generic_state_transitions() {
    // Regression guard: `Executing` used to say generic filler ("Voy a
    // hacerlo ahora") that immediately preceded `ToolStarted` saying
    // something specific about the same action — pure noise. Only
    // `Thinking` and `Verifying` have a real silent gap behind them.
    let mut narrator = TemplateNarrator::new();

    for state in [
        TurnState::Receiving,
        TurnState::Planning,
        TurnState::Executing,
        TurnState::Responding,
    ] {
        assert_eq!(
            narrator.narrate(&Event::StateChanged {
                task_id: task_id(),
                state
            }),
            None
        );
    }
}

#[test]
fn does_not_narrate_noisy_bookkeeping_events() {
    let mut narrator = TemplateNarrator::new();

    assert_eq!(
        narrator.narrate(&Event::TokensUsed {
            task_id: task_id(),
            llm_call_id: llm_call_id(),
            call_index: 1,
            prompt_tokens: Some(10),
            completion_tokens: Some(5),
        }),
        None
    );
    assert_eq!(
        narrator.narrate(&Event::LlmStarted {
            task_id: task_id(),
            llm_call_id: llm_call_id(),
            call_index: 1,
            images: 0,
            provider: "ollama".to_string(),
            model: "gemma4:12b".to_string(),
        }),
        None
    );
    assert_eq!(
        narrator.narrate(&Event::LlmCompleted {
            task_id: task_id(),
            llm_call_id: llm_call_id(),
            call_index: 1,
            duration: Duration::from_millis(1),
            provider: "ollama".to_string(),
            model: "gemma4:12b".to_string(),
        }),
        None
    );
}

#[test]
fn narrates_a_retry() {
    let mut narrator = TemplateNarrator::new();

    let phrase = narrator.narrate(&Event::Retrying {
        task_id: task_id(),
        attempt: 1,
        error: "network hiccup".to_string(),
    });

    assert!(phrase.is_some());
}

#[test]
fn narrates_a_failed_tool_result_but_not_a_successful_one() {
    let mut narrator = TemplateNarrator::new();

    let ok = narrator.narrate(&Event::ToolCompleted {
        task_id: task_id(),
        tool_call_index: 1,
        name: "execute_command".to_string(),
        duration: Duration::from_millis(1),
        output: "did the thing".to_string(),
        images: 0,
        arguments: "{}".to_string(),
        mutated: false,
    });
    assert_eq!(ok, None);

    let failed = narrator.narrate(&Event::ToolCompleted {
        task_id: task_id(),
        tool_call_index: 2,
        name: "execute_command".to_string(),
        duration: Duration::from_millis(1),
        output: "ERROR: boom".to_string(),
        images: 0,
        arguments: "{}".to_string(),
        mutated: false,
    });
    assert!(failed.is_some());
}
