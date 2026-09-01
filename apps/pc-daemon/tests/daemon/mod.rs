use device_capabilities::capabilities::execute_command::ExecuteCommandTool;
use device_capabilities::registry::CapabilityRegistry;
use device_protocol::{DeviceMessage, ErrorCode, NalaMessage, Outcome, RejectReason};
use pc_daemon::config::DeviceIdentity;
use pc_daemon::daemon::{SessionOutcome, run_session};
use pc_daemon::overlay_channel::OverlayChannel;

use crate::fake_computer::FakeComputer;
use crate::fake_wire::FakeDeviceWire;

fn identity() -> DeviceIdentity {
    DeviceIdentity {
        device_id: "pc-1".to_string(),
        name: "pc".to_string(),
        platform: "windows".to_string(),
        token: "secret".to_string(),
    }
}

fn registry_with_execute_command() -> CapabilityRegistry {
    let mut registry = CapabilityRegistry::new();
    registry.register(ExecuteCommandTool::new(FakeComputer::new()));
    registry
}

#[test]
fn the_daemon_announces_its_capabilities_in_its_hello() {
    let mut wire = FakeDeviceWire::new(vec![]);
    let mut registry = registry_with_execute_command();
    let overlay = OverlayChannel::new();

    run_session(&mut wire, &mut registry, &identity(), &overlay).expect("session should not error");

    match &wire.sent[0] {
        DeviceMessage::Hello { capabilities, .. } => {
            assert_eq!(capabilities.len(), 1);
            assert_eq!(capabilities[0].name, "execute_command");
        }
        other => panic!("expected Hello, got {other:?}"),
    }
}

#[test]
fn an_invoke_runs_the_capability_and_returns_a_result_with_the_same_request_id() {
    let mut wire = FakeDeviceWire::new(vec![NalaMessage::Invoke {
        request_id: "req-1".to_string(),
        capability: "execute_command".to_string(),
        arguments: r#"{"command":"start chrome"}"#.to_string(),
    }]);
    let mut registry = registry_with_execute_command();
    let overlay = OverlayChannel::new();

    run_session(&mut wire, &mut registry, &identity(), &overlay).expect("session should not error");

    match &wire.sent[1] {
        DeviceMessage::Result {
            request_id,
            outcome,
        } => {
            assert_eq!(request_id, "req-1");
            assert!(matches!(outcome, Outcome::Ok { .. }));
        }
        other => panic!("expected Result, got {other:?}"),
    }
}

#[test]
fn a_failing_capability_returns_an_error_result_instead_of_dropping_the_connection() {
    let mut wire = FakeDeviceWire::new(vec![
        NalaMessage::Invoke {
            request_id: "req-1".to_string(),
            capability: "execute_command".to_string(),
            arguments: "not json".to_string(),
        },
        NalaMessage::Invoke {
            request_id: "req-2".to_string(),
            capability: "execute_command".to_string(),
            arguments: r#"{"command":"start chrome"}"#.to_string(),
        },
    ]);
    let mut registry = registry_with_execute_command();
    let overlay = OverlayChannel::new();

    run_session(&mut wire, &mut registry, &identity(), &overlay).expect("session should not error");

    match &wire.sent[1] {
        DeviceMessage::Result { outcome, .. } => {
            assert!(matches!(
                outcome,
                Outcome::Err {
                    code: ErrorCode::BadArguments,
                    ..
                }
            ));
        }
        other => panic!("expected Result, got {other:?}"),
    }
    // The second, valid Invoke still gets answered — one bad call doesn't
    // end the session.
    match &wire.sent[2] {
        DeviceMessage::Result { request_id, .. } => assert_eq!(request_id, "req-2"),
        other => panic!("expected Result, got {other:?}"),
    }
}

#[test]
fn an_unknown_capability_returns_not_found() {
    let mut wire = FakeDeviceWire::new(vec![NalaMessage::Invoke {
        request_id: "req-1".to_string(),
        capability: "does_not_exist".to_string(),
        arguments: "{}".to_string(),
    }]);
    let mut registry = registry_with_execute_command();
    let overlay = OverlayChannel::new();

    run_session(&mut wire, &mut registry, &identity(), &overlay).expect("session should not error");

    match &wire.sent[1] {
        DeviceMessage::Result { outcome, .. } => {
            assert!(matches!(
                outcome,
                Outcome::Err {
                    code: ErrorCode::NotFound,
                    ..
                }
            ));
        }
        other => panic!("expected Result, got {other:?}"),
    }
}

#[test]
fn a_ping_is_answered_with_a_pong() {
    let mut wire = FakeDeviceWire::new(vec![NalaMessage::Ping { id: 7 }]);
    let mut registry = registry_with_execute_command();
    let overlay = OverlayChannel::new();

    run_session(&mut wire, &mut registry, &identity(), &overlay).expect("session should not error");

    match &wire.sent[1] {
        DeviceMessage::Pong { id } => assert_eq!(*id, 7),
        other => panic!("expected Pong, got {other:?}"),
    }
}

#[test]
fn a_reject_ends_the_session_instead_of_retrying_forever() {
    let mut wire = FakeDeviceWire::new(vec![
        NalaMessage::Reject {
            reason: RejectReason::BadToken,
        },
        // Never reached if the session correctly stops at Reject.
        NalaMessage::Ping { id: 1 },
    ]);
    let mut registry = registry_with_execute_command();
    let overlay = OverlayChannel::new();

    let outcome =
        run_session(&mut wire, &mut registry, &identity(), &overlay).expect("should not error");

    assert!(matches!(
        outcome,
        SessionOutcome::Rejected(RejectReason::BadToken)
    ));
    // Only the Hello was sent — no Pong for the Ping that follows the
    // Reject in the script.
    assert_eq!(wire.sent.len(), 1);
}

#[test]
fn the_session_loop_ends_when_the_connection_closes() {
    let mut wire = FakeDeviceWire::new(vec![]);
    let mut registry = registry_with_execute_command();
    let overlay = OverlayChannel::new();

    let outcome =
        run_session(&mut wire, &mut registry, &identity(), &overlay).expect("should not error");

    assert!(matches!(outcome, SessionOutcome::Closed));
}
