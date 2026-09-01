use crate::fake_computer::FakeComputer;
use device_capabilities::capabilities::execute_command::ExecuteCommandTool;
use device_capabilities::capabilities::open_app::OpenAppTool;
use device_capabilities::registry::CapabilityRegistry;
use device_protocol::{ErrorCode, Outcome};

#[test]
fn the_registry_lists_every_registered_capability() {
    let mut registry = CapabilityRegistry::new();
    registry.register(ExecuteCommandTool::new(FakeComputer::new()));
    registry.register(OpenAppTool::new(FakeComputer::new()));

    let mut names: Vec<String> = registry
        .definitions()
        .into_iter()
        .map(|definition| definition.name)
        .collect();
    names.sort();

    assert_eq!(
        names,
        vec!["execute_command".to_string(), "open_app".to_string()]
    );
}

#[test]
fn invoking_an_unknown_capability_is_a_not_found_error() {
    let mut registry = CapabilityRegistry::new();
    registry.register(ExecuteCommandTool::new(FakeComputer::new()));

    let outcome = registry.invoke("does_not_exist", "{}");

    assert!(matches!(
        outcome,
        Outcome::Err {
            code: ErrorCode::NotFound,
            ..
        }
    ));
}

#[test]
fn invoking_a_capability_outside_the_allowlist_is_denied() {
    let mut registry = CapabilityRegistry::with_allowlist(["open_app"]);
    registry.register(ExecuteCommandTool::new(FakeComputer::new()));
    registry.register(OpenAppTool::new(FakeComputer::new()));

    let outcome = registry.invoke("execute_command", r#"{"command":"start chrome"}"#);

    assert!(matches!(
        outcome,
        Outcome::Err {
            code: ErrorCode::Denied,
            ..
        }
    ));
}

#[test]
fn a_capability_outside_the_allowlist_is_not_announced() {
    let mut registry = CapabilityRegistry::with_allowlist(["open_app"]);
    registry.register(ExecuteCommandTool::new(FakeComputer::new()));
    registry.register(OpenAppTool::new(FakeComputer::new()));

    let names: Vec<String> = registry
        .definitions()
        .into_iter()
        .map(|definition| definition.name)
        .collect();

    assert_eq!(names, vec!["open_app".to_string()]);
}

#[test]
fn a_mutating_capability_reports_mutated_in_its_outcome() {
    let mut registry = CapabilityRegistry::new();
    registry.register(ExecuteCommandTool::new(FakeComputer::new()));

    let outcome = registry.invoke("execute_command", r#"{"command":"start chrome"}"#);

    match outcome {
        Outcome::Ok { mutated, .. } => assert!(mutated),
        Outcome::Err { message, .. } => panic!("expected Ok, got Err: {message}"),
    }
}
