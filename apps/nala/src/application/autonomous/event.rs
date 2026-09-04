//! The generic representation of something that happened outside a user's
//! request: a device report, a timer firing, an eventual Home Assistant
//! notification. Deliberately string-typed (`source`/`kind`, not an enum
//! of every event this crate knows about) so a new event source can start
//! publishing a new kind of event without a change here — see
//! `EventPolicy` for where those strings get interpreted.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Identifies one `AutonomousEvent`, for correlating the
/// `AutonomousEvent*` narration events it produces back to it. Same
/// construction scheme as `agent_protocol::TaskId` (current millis plus a
/// monotonic counter) for the same reason: Nala is single-process, so
/// global uniqueness isn't required.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AutonomousEventId(String);

impl AutonomousEventId {
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
        Self(format!("{millis}-{sequence}"))
    }
}

impl Default for AutonomousEventId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AutonomousEventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// One thing that happened outside a user's request. `source` identifies
/// where it came from (a device id, `"timer"`, `"home-assistant"`, ...);
/// `kind` identifies what happened (`"battery_low"`, `"button_pressed"`,
/// `"device_connected"`, ...); `payload` carries whatever detail the
/// source wants to attach, opaque to everything upstream of `EventPolicy`.
#[derive(Debug, Clone, PartialEq)]
pub struct AutonomousEvent {
    pub id: AutonomousEventId,
    pub source: String,
    pub kind: String,
    pub payload: serde_json::Value,
    /// Unix milliseconds, so two events observed close together can still
    /// be ordered even if the queue doesn't preserve arrival order (it
    /// currently does, but this doesn't rely on that).
    pub observed_at_millis: u64,
}

impl AutonomousEvent {
    pub fn new(
        source: impl Into<String>,
        kind: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        let observed_at_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0);

        Self {
            id: AutonomousEventId::new(),
            source: source.into(),
            kind: kind.into(),
            payload,
            observed_at_millis,
        }
    }

    /// Whether two events would look like the same occurrence to a
    /// consumer -- same source, kind, and payload -- used by the queue to
    /// drop an exact duplicate that's already waiting. Deliberately
    /// ignores `id`/`observed_at_millis`, which are always unique.
    pub fn is_duplicate_of(&self, other: &AutonomousEvent) -> bool {
        self.source == other.source && self.kind == other.kind && self.payload == other.payload
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_events_with_the_same_source_kind_and_payload_are_duplicates() {
        let payload = serde_json::json!({"percent": 9});
        let a = AutonomousEvent::new("esp32-bedroom", "battery_low", payload.clone());
        let b = AutonomousEvent::new("esp32-bedroom", "battery_low", payload);

        assert!(a.is_duplicate_of(&b));
    }

    #[test]
    fn events_with_different_payloads_are_not_duplicates() {
        let a = AutonomousEvent::new(
            "esp32-bedroom",
            "battery_low",
            serde_json::json!({"percent": 9}),
        );
        let b = AutonomousEvent::new(
            "esp32-bedroom",
            "battery_low",
            serde_json::json!({"percent": 50}),
        );

        assert!(!a.is_duplicate_of(&b));
    }

    #[test]
    fn events_from_different_sources_are_not_duplicates() {
        let payload = serde_json::json!({"percent": 9});
        let a = AutonomousEvent::new("esp32-bedroom", "battery_low", payload.clone());
        let b = AutonomousEvent::new("esp32-kitchen", "battery_low", payload);

        assert!(!a.is_duplicate_of(&b));
    }
}
