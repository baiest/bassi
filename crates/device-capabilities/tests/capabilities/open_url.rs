use crate::fake_computer::FakeComputer;
use device_capabilities::Capability;
use device_capabilities::capabilities::open_url::{OpenUrlArgs, OpenUrlTool};
use schemars::schema_for;

#[test]
fn opens_a_valid_http_url() {
    let computer = FakeComputer::new();
    let mut tool: OpenUrlTool<FakeComputer> = OpenUrlTool::new(computer);

    let args = OpenUrlArgs {
        url: "https://example.com".to_string(),
    };

    let result = tool.execute(args);

    assert!(result.is_ok());
    // `rundll32 url.dll,FileProtocolHandler`, not `cmd /C start` or
    // `explorer` — both of those were observed failing (denied, or opening
    // File Explorer instead of the browser) on a machine where this
    // rundll32 invocation, the standard scripted way to open a URL through
    // Windows's own URL protocol handler, opened the default browser
    // correctly. See BAS-52.
    assert_eq!(
        tool.computer.executed_command,
        Some("rundll32 url.dll,FileProtocolHandler \"https://example.com\"".to_string())
    );
}

#[test]
fn opens_a_plain_http_url() {
    let computer = FakeComputer::new();
    let mut tool: OpenUrlTool<FakeComputer> = OpenUrlTool::new(computer);

    let args = OpenUrlArgs {
        url: "http://example.com".to_string(),
    };

    let result = tool.execute(args);

    assert!(result.is_ok());
}

#[test]
fn rejects_a_url_without_an_http_scheme_without_touching_the_computer() {
    let computer = FakeComputer::new();
    let mut tool: OpenUrlTool<FakeComputer> = OpenUrlTool::new(computer);

    let args = OpenUrlArgs {
        url: "javascript:alert(1)".to_string(),
    };

    let result = tool.execute(args);

    assert!(result.is_err());
    assert_eq!(tool.computer.executed_command, None);
}

#[test]
fn rejects_a_url_containing_a_quote_without_touching_the_computer() {
    let computer = FakeComputer::new();
    let mut tool: OpenUrlTool<FakeComputer> = OpenUrlTool::new(computer);

    let args = OpenUrlArgs {
        url: "https://example.com/\" & calc.exe".to_string(),
    };

    let result = tool.execute(args);

    assert!(result.is_err());
    assert_eq!(tool.computer.executed_command, None);
}

#[test]
fn returns_error_when_computer_fails() {
    let mut computer = FakeComputer::new();
    computer.should_fail = true;
    let mut tool = OpenUrlTool::new(computer);

    let args = OpenUrlArgs {
        url: "https://example.com".to_string(),
    };

    let result = tool.execute(args);

    assert!(result.is_err());
}

#[test]
fn published_schema_matches_the_derived_schema_of_args() {
    let expected =
        serde_json::to_value(schema_for!(OpenUrlArgs)).expect("schema should serialize to JSON");

    let definition = OpenUrlTool::<FakeComputer>::definition();

    assert_eq!(definition.parameters, expected);
}
