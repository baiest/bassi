use nala::application::tools::Tool;
use nala::application::tools::ping::PingTool;

#[test]
fn responds_pong() {
    let mut tool = PingTool::new();

    let result = tool.execute(()).expect("ping should not fail");

    assert_eq!(result, "pong");
}

#[test]
fn has_no_context() {
    let mut tool = PingTool::new();

    let context = tool.context().expect("ping context should not fail");

    assert_eq!(context, "");
}

#[test]
fn ignores_any_arguments() {
    let result = PingTool::parse_arguments("irrelevant");

    assert!(result.is_ok());
}
