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
    // `cmd /C start "" <url>` is tried first: `rundll32
    // url.dll,FileProtocolHandler` was the original choice (see BAS-52),
    // but was later found to silently do nothing on some machines --
    // exiting 0 without actually opening a browser -- while `start`
    // reliably worked there. It stays as the fallback below for the
    // machine where `start` was itself denied outright. See BAS-59.
    assert_eq!(
        tool.computer.executed_command,
        Some("start \"\" \"https://example.com\"".to_string())
    );
}

#[test]
fn falls_back_to_rundll32_when_start_fails() {
    let mut computer = FakeComputer::new();
    computer.fail_when_command_contains = Some("start".to_string());
    let mut tool: OpenUrlTool<FakeComputer> = OpenUrlTool::new(computer);

    let args = OpenUrlArgs {
        url: "https://example.com".to_string(),
    };

    let result = tool.execute(args);

    assert!(result.is_ok());
    assert_eq!(
        tool.computer.commands_run,
        vec!["rundll32 url.dll,FileProtocolHandler \"https://example.com\"".to_string()]
    );
}

#[test]
fn the_success_message_does_not_assert_the_browser_actually_opened() {
    // `rundll32 url.dll,FileProtocolHandler` is fire-and-forget: it hands
    // the URL to the shell and returns 0 almost instantly, whether or not
    // a browser window actually appears (a missing default browser, or a
    // system dialog, can silently swallow it). The result text must not
    // claim a fact `execute()` has no evidence for -- see BAS-58.
    let computer = FakeComputer::new();
    let mut tool: OpenUrlTool<FakeComputer> = OpenUrlTool::new(computer);

    let args = OpenUrlArgs {
        url: "https://youtube.com".to_string(),
    };

    let result = tool.execute(args).unwrap();

    assert!(
        !result.contains("Opened https://youtube.com"),
        "the message must not assert the URL was opened, got: {result:?}"
    );
    assert!(
        result.contains("does not confirm"),
        "the message must flag that running the command doesn't confirm the outcome, got: {result:?}"
    );
}

#[test]
fn the_success_message_does_not_assert_the_browser_actually_opened() {
    // `rundll32 url.dll,FileProtocolHandler` is fire-and-forget: it hands
    // the URL to the shell and returns 0 almost instantly, whether or not
    // a browser window actually appears (a missing default browser, or a
    // system dialog, can silently swallow it). The result text must not
    // claim a fact `execute()` has no evidence for -- see BAS-58.
    let computer = FakeComputer::new();
    let mut tool: OpenUrlTool<FakeComputer> = OpenUrlTool::new(computer);

    let args = OpenUrlArgs {
        url: "https://youtube.com".to_string(),
    };

    let result = tool.execute(args).unwrap();

    assert!(
        !result.contains("Opened https://youtube.com"),
        "the message must not assert the URL was opened, got: {result:?}"
    );
    assert!(
        result.contains("does not confirm"),
        "the message must flag that running the command doesn't confirm the outcome, got: {result:?}"
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
