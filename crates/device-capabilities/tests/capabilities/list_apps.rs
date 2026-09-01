use crate::fake_computer::FakeComputer;
use device_capabilities::Capability;
use device_capabilities::capabilities::list_apps::{ListAppsTool, parse_start_apps};

#[test]
fn parses_a_json_array_sorting_and_deduping_names() {
    let json = r#"[
        {"Name":"Notepad","AppID":"notepad.exe"},
        {"Name":"Calculator","AppID":"calc.exe"},
        {"Name":"Notepad","AppID":"notepad2.exe"}
    ]"#;

    let names = parse_start_apps(json).expect("should parse");

    assert_eq!(names, vec!["Calculator".to_string(), "Notepad".to_string()]);
}

#[test]
fn parses_a_single_json_object_edge_case() {
    let json = r#"{"Name":"Notepad","AppID":"notepad.exe"}"#;

    let names = parse_start_apps(json).expect("should parse");

    assert_eq!(names, vec!["Notepad".to_string()]);
}

#[test]
fn parses_an_empty_array_as_no_apps() {
    let names = parse_start_apps("[]").expect("should parse");

    assert!(names.is_empty());
}

#[test]
fn rejects_malformed_json_with_a_clear_error() {
    let result = parse_start_apps("not json");

    assert!(result.is_err());
}

#[test]
fn lists_apps_via_the_computer() {
    let mut computer = FakeComputer::new();
    computer.output = r#"[{"Name":"Notepad","AppID":"notepad.exe"}]"#.to_string();
    let mut tool: ListAppsTool<FakeComputer> = ListAppsTool::new(computer);

    let result = tool.execute(()).expect("should execute");

    assert!(result.contains("Notepad"));
    assert!(
        tool.computer
            .executed_command
            .unwrap()
            .contains("Get-StartApps")
    );
}

#[test]
fn returns_error_when_computer_fails() {
    let mut computer = FakeComputer::new();
    computer.should_fail = true;
    let mut tool: ListAppsTool<FakeComputer> = ListAppsTool::new(computer);

    let result = tool.execute(());

    assert!(result.is_err());
}

#[test]
fn truncates_and_notes_when_over_the_cap() {
    let mut computer = FakeComputer::new();
    let apps: Vec<String> = (0..200)
        .map(|i| format!(r#"{{"Name":"App{i:03}","AppID":"app{i}.exe"}}"#))
        .collect();
    computer.output = format!("[{}]", apps.join(","));
    let mut tool: ListAppsTool<FakeComputer> = ListAppsTool::new(computer);

    let result = tool.execute(()).expect("should execute");

    assert!(result.contains("more, ask about a specific app"));
    assert_eq!(result.lines().filter(|l| l.starts_with("App")).count(), 150);
}
