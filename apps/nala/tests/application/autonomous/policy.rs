use nala::application::autonomous::event::AutonomousEvent;
use nala::application::autonomous::policy::{EventDecision, EventPolicy, RuleBasedPolicy};

fn policy() -> RuleBasedPolicy {
    RuleBasedPolicy::new(
        ["battery_low", "button_pressed"],
        ["heartbeat", "telemetry"],
    )
}

#[test]
fn an_event_kind_on_the_ignore_list_is_ignored() {
    let event = AutonomousEvent::new("esp32-bedroom", "heartbeat", serde_json::json!({}));

    let decision = policy().decide(&event);

    match decision {
        EventDecision::Ignore { .. } => {}
        EventDecision::Act { .. } => panic!("expected Ignore"),
    }
}

#[test]
fn an_event_kind_on_the_act_list_is_acted_on_with_a_rendered_prompt() {
    let event = AutonomousEvent::new(
        "esp32-bedroom",
        "battery_low",
        serde_json::json!({"percent": 9}),
    );

    let decision = policy().decide(&event);

    match decision {
        EventDecision::Act { prompt } => {
            assert!(prompt.contains("battery_low"));
            assert!(prompt.contains("esp32-bedroom"));
            assert!(prompt.contains("9"));
        }
        EventDecision::Ignore { .. } => panic!("expected Act"),
    }
}

#[test]
fn an_unknown_event_kind_defaults_to_ignore() {
    let event = AutonomousEvent::new(
        "esp32-bedroom",
        "some_new_unrecognised_kind",
        serde_json::json!({}),
    );

    let decision = policy().decide(&event);

    match decision {
        EventDecision::Ignore { .. } => {}
        EventDecision::Act { .. } => panic!("an unknown event kind must never trigger an LLM call"),
    }
}
