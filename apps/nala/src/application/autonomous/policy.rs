//! Where Nala decides whether an `AutonomousEvent` is worth an LLM call.
//! Not every event is: a heartbeat or a routine telemetry point should
//! never reach the model, only things that could plausibly need a
//! response or an action. Kept deliberately simple -- a static
//! ignore/act list, not a learned or LLM-backed classifier -- since the
//! goal here is the place this decision lives, not a sophisticated
//! planner.

use std::collections::HashSet;

use crate::application::autonomous::event::AutonomousEvent;

/// What should happen with one `AutonomousEvent`.
///
/// `Notify` and `Defer` are deliberately not modeled yet: there is no
/// channel today for pushing a notification to a connected user client
/// (each websocket connection owns its own `Assistant`, with no broadcast
/// path out), and no scheduler to defer an event onto. Add them here, and
/// give the event loop a matching arm, once those exist.
#[derive(Debug, Clone, PartialEq)]
pub enum EventDecision {
    /// No action needed -- no LLM call is made for this event.
    Ignore { reason: String },
    /// Worth reasoning about. `prompt` is what gets handed to the existing
    /// agent loop as the turn's input.
    Act { prompt: String },
}

pub trait EventPolicy: Send {
    fn decide(&self, event: &AutonomousEvent) -> EventDecision;
}

/// The default `EventPolicy`: a static allowlist of event kinds worth
/// acting on, plus an explicit ignore-list for common noisy kinds
/// (documented for operators, though anything not on the allowlist is
/// ignored regardless). Unknown kinds -- anything that isn't on either
/// list -- default to `Ignore`: an unrecognised event must never burn an
/// LLM call on its own.
pub struct RuleBasedPolicy {
    act_on: HashSet<String>,
    #[allow(dead_code)]
    // documents intent; ignoring is already the default for anything not in `act_on`
    ignore: HashSet<String>,
}

impl RuleBasedPolicy {
    pub fn new<A, I>(act_on: A, ignore: I) -> Self
    where
        A: IntoIterator,
        A::Item: Into<String>,
        I: IntoIterator,
        I::Item: Into<String>,
    {
        Self {
            act_on: act_on.into_iter().map(Into::into).collect(),
            ignore: ignore.into_iter().map(Into::into).collect(),
        }
    }

    /// The kinds this crate recognizes today as worth reasoning about.
    /// Extend this list as new event sources (a real ESP32, Home
    /// Assistant, a scheduler) come online -- it never needs a core
    /// change to add a kind, only an update here.
    pub fn default_kinds() -> Self {
        Self::new(
            ["battery_low", "button_pressed"],
            [
                "heartbeat",
                "telemetry",
                "device_connected",
                "device_disconnected",
            ],
        )
    }

    fn render_prompt(event: &AutonomousEvent) -> String {
        format!(
            "An autonomous event occurred.\nSource: {}\nKind: {}\nDetails: {}\n\nDetermine whether any action or response is required, and act on it if so.",
            event.source, event.kind, event.payload
        )
    }
}

impl EventPolicy for RuleBasedPolicy {
    fn decide(&self, event: &AutonomousEvent) -> EventDecision {
        if self.act_on.contains(&event.kind) {
            EventDecision::Act {
                prompt: Self::render_prompt(event),
            }
        } else {
            EventDecision::Ignore {
                reason: format!("'{}' is not on the act-on list", event.kind),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_kinds_acts_on_battery_low() {
        let policy = RuleBasedPolicy::default_kinds();
        let event = AutonomousEvent::new("esp32-bedroom", "battery_low", serde_json::json!({}));

        assert!(matches!(policy.decide(&event), EventDecision::Act { .. }));
    }

    #[test]
    fn default_kinds_ignores_a_heartbeat() {
        let policy = RuleBasedPolicy::default_kinds();
        let event = AutonomousEvent::new("esp32-bedroom", "heartbeat", serde_json::json!({}));

        assert!(matches!(
            policy.decide(&event),
            EventDecision::Ignore { .. }
        ));
    }
}
