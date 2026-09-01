use crate::fake_device::FakeDevice;
use device_protocol::{ErrorCode, Outcome};
use nala::application::tools::device_toolset::DeviceToolset;

#[test]
fn a_device_toolset_publishes_the_definitions_it_was_given() {
    let device = FakeDevice::new("pc")
        .with_capability("open_app", "Opens an app")
        .with_capability("volume", "Changes the volume");
    let toolset = DeviceToolset::new(device);

    let definitions = toolset.definitions();

    assert_eq!(definitions.len(), 2);
    assert_eq!(definitions[0].description, "Opens an app");
}

#[test]
fn capability_names_are_published_prefixed_with_the_device_name() {
    let device = FakeDevice::new("pc").with_capability("open_app", "Opens an app");
    let toolset = DeviceToolset::new(device);

    let definitions = toolset.definitions();

    assert_eq!(definitions[0].name, "pc_open_app");
}

#[test]
fn handles_recognizes_only_its_own_prefixed_capabilities() {
    let device = FakeDevice::new("pc").with_capability("open_app", "Opens an app");
    let toolset = DeviceToolset::new(device);

    assert!(toolset.handles("pc_open_app"));
    assert!(!toolset.handles("open_app"));
    assert!(!toolset.handles("pc_volume"));
}

#[test]
fn a_device_error_result_becomes_a_tool_error_not_a_panic() {
    let device = FakeDevice::new("pc")
        .with_capability("open_app", "Opens an app")
        .returning(Outcome::Err {
            code: ErrorCode::Failed,
            message: "boom".to_string(),
        });
    let mut toolset = DeviceToolset::new(device);

    let outcome = toolset.call("pc_open_app", "{}");

    assert!(outcome.text.contains("boom"));
    assert!(!outcome.mutated);
}

#[test]
fn a_devices_mutated_result_sets_mutated_on_the_tool_outcome() {
    let device = FakeDevice::new("pc")
        .with_capability("open_app", "Opens an app")
        .returning(Outcome::Ok {
            text: "opened".to_string(),
            mutated: true,
        });
    let mut toolset = DeviceToolset::new(device);

    let outcome = toolset.call("pc_open_app", r#"{"app":"Spotify"}"#);

    assert_eq!(outcome.text, "opened");
    assert!(outcome.mutated);
}

#[test]
fn calling_strips_the_device_prefix_before_invoking() {
    let device = FakeDevice::new("pc").with_capability("open_app", "Opens an app");
    let mut toolset = DeviceToolset::new(device);

    toolset.call("pc_open_app", r#"{"app":"Spotify"}"#);

    assert_eq!(
        toolset.device().last_invoke,
        Some(("open_app".to_string(), r#"{"app":"Spotify"}"#.to_string()))
    );
}
