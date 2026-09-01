//! Wire protocol between Nala and a device daemon (e.g. the PC daemon) —
//! separate from `agent-protocol` (user↔agent) because these are two
//! different conversations: a device announces capabilities and gets
//! invoked, it never sends user input or receives replies. Kept
//! dependency-free of both `nala` and any daemon crate.

use serde::{Deserialize, Serialize};

/// Bumped on any breaking change to `DeviceMessage`/`NalaMessage`. Compared,
/// never assumed — see `RejectReason::UnsupportedVersion`.
pub const PROTOCOL_VERSION: u16 = 1;

/// One capability a device announces in its `Hello`. Field-for-field what
/// `nala`'s `ToolDefinition` needs, so the conversion on Nala's side is a
/// straight map — this crate doesn't depend on `nala` to define it in terms
/// of that type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Why Nala refused a device's `Hello`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectReason {
    UnsupportedVersion,
    BadToken,
    DuplicateDevice,
}

/// Why a capability invocation failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    NotFound,
    BadArguments,
    Failed,
    Denied,
    Timeout,
}

/// What running a capability produced, mirroring `nala`'s `ToolOutcome`
/// closely enough that converting one into the other is lossless for the
/// fields that matter to the agent loop's verification gate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum Outcome {
    Ok { text: String, mutated: bool },
    Err { code: ErrorCode, message: String },
}

/// State a device daemon reports about itself, for a local overlay
/// subscriber to render — Nala never sees these values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceState {
    Idle,
    Listening,
    Thinking,
    Executing,
    Speaking,
    Error,
}

/// A message a device daemon sends to Nala.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DeviceMessage {
    Hello {
        protocol_version: u16,
        device_id: String,
        name: String,
        platform: String,
        token: String,
        capabilities: Vec<CapabilityDefinition>,
    },
    Result {
        request_id: String,
        outcome: Outcome,
    },
    Pong {
        id: u64,
    },
}

/// A message Nala sends to a connected device daemon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NalaMessage {
    Welcome {
        session_id: String,
        heartbeat_interval_ms: u64,
    },
    Reject {
        reason: RejectReason,
    },
    Invoke {
        request_id: String,
        capability: String,
        arguments: String,
    },
    Ping {
        id: u64,
    },
    State {
        state: DeviceState,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_capability() -> CapabilityDefinition {
        CapabilityDefinition {
            name: "open_app".to_string(),
            description: "Opens an app".to_string(),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
        }
    }

    #[test]
    fn a_hello_round_trips_through_json() {
        let message = DeviceMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            device_id: "pc-1".to_string(),
            name: "pc".to_string(),
            platform: "windows".to_string(),
            token: "secret".to_string(),
            capabilities: vec![sample_capability()],
        };

        let json = serde_json::to_string(&message).unwrap();
        let decoded: DeviceMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, message);
    }

    #[test]
    fn an_invoke_round_trips_through_json() {
        let message = NalaMessage::Invoke {
            request_id: "req-1".to_string(),
            capability: "open_app".to_string(),
            arguments: "{\"app\":\"Spotify\"}".to_string(),
        };

        let json = serde_json::to_string(&message).unwrap();
        let decoded: NalaMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, message);
    }

    #[test]
    fn a_successful_result_round_trips_with_its_mutated_flag() {
        let message = DeviceMessage::Result {
            request_id: "req-1".to_string(),
            outcome: Outcome::Ok {
                text: "opened Spotify".to_string(),
                mutated: true,
            },
        };

        let json = serde_json::to_string(&message).unwrap();
        let decoded: DeviceMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, message);
        match decoded {
            DeviceMessage::Result {
                outcome: Outcome::Ok { mutated, .. },
                ..
            } => assert!(mutated),
            _ => panic!("expected Result(Ok)"),
        }
    }

    #[test]
    fn an_error_result_round_trips_with_its_code() {
        let message = DeviceMessage::Result {
            request_id: "req-1".to_string(),
            outcome: Outcome::Err {
                code: ErrorCode::Timeout,
                message: "no response".to_string(),
            },
        };

        let json = serde_json::to_string(&message).unwrap();
        let decoded: DeviceMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, message);
        match decoded {
            DeviceMessage::Result {
                outcome: Outcome::Err { code, .. },
                ..
            } => assert_eq!(code, ErrorCode::Timeout),
            _ => panic!("expected Result(Err)"),
        }
    }

    #[test]
    fn a_hello_from_an_unknown_protocol_version_still_deserializes() {
        // Nala must be able to read a Hello carrying a version it doesn't
        // support, so it can compare and reply Reject instead of failing to
        // parse the handshake at all.
        let message = DeviceMessage::Hello {
            protocol_version: 9999,
            device_id: "pc-1".to_string(),
            name: "pc".to_string(),
            platform: "windows".to_string(),
            token: "secret".to_string(),
            capabilities: vec![],
        };

        let json = serde_json::to_string(&message).unwrap();
        let decoded: DeviceMessage = serde_json::from_str(&json).unwrap();

        match decoded {
            DeviceMessage::Hello {
                protocol_version, ..
            } => assert_eq!(protocol_version, 9999),
            _ => panic!("expected Hello"),
        }
    }

    #[test]
    fn an_unknown_message_type_is_a_deserialization_error_not_a_panic() {
        let result: Result<DeviceMessage, _> =
            serde_json::from_str(r#"{"type":"NotARealVariant"}"#);

        assert!(result.is_err());
    }
}
