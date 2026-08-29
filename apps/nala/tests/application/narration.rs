use std::time::Duration;

use nala::application::narration::TemplateNarrator;
use nala::ports::events::{Event, TurnState};
use nala::ports::narrator::Narrator;

#[test]
fn narrates_a_known_tool_starting() {
    let mut narrator = TemplateNarrator::new();

    let phrase = narrator.narrate(&Event::ToolStarted {
        name: "screenshot".to_string(),
        arguments: "{}".to_string(),
    });

    assert_eq!(phrase, Some("Déjame ver la pantalla.".to_string()));
}

#[test]
fn rotates_phrases_for_the_same_state_across_a_turn() {
    let mut narrator = TemplateNarrator::new();

    let first = narrator
        .narrate(&Event::StateChanged {
            state: TurnState::Thinking,
        })
        .expect("first Thinking should narrate");

    // Something else happens in between, so the repeat-suppression rule
    // doesn't swallow the second Thinking.
    narrator.narrate(&Event::StateChanged {
        state: TurnState::Executing,
    });

    let second = narrator
        .narrate(&Event::StateChanged {
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
            state: TurnState::Thinking,
        })
        .expect("first Thinking should narrate");

    let second = narrator.narrate(&Event::StateChanged {
        state: TurnState::Thinking,
    });

    assert_eq!(second, None);
}

#[test]
fn does_not_narrate_noisy_bookkeeping_events() {
    let mut narrator = TemplateNarrator::new();

    assert_eq!(
        narrator.narrate(&Event::TokensUsed {
            prompt_tokens: Some(10),
            completion_tokens: Some(5),
        }),
        None
    );
    assert_eq!(narrator.narrate(&Event::LlmStarted { images: 0 }), None);
    assert_eq!(
        narrator.narrate(&Event::LlmCompleted {
            duration: Duration::from_millis(1),
        }),
        None
    );
}

#[test]
fn narrates_a_retry() {
    let mut narrator = TemplateNarrator::new();

    let phrase = narrator.narrate(&Event::Retrying {
        attempt: 1,
        error: "network hiccup".to_string(),
    });

    assert!(phrase.is_some());
}

#[test]
fn narrates_a_failed_tool_result_but_not_a_successful_one() {
    let mut narrator = TemplateNarrator::new();

    let ok = narrator.narrate(&Event::ToolCompleted {
        name: "execute_command".to_string(),
        duration: Duration::from_millis(1),
        output: "did the thing".to_string(),
        images: 0,
    });
    assert_eq!(ok, None);

    let failed = narrator.narrate(&Event::ToolCompleted {
        name: "execute_command".to_string(),
        duration: Duration::from_millis(1),
        output: "ERROR: boom".to_string(),
        images: 0,
    });
    assert!(failed.is_some());
}
