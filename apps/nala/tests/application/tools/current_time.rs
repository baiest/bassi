use crate::fake_wall_clock::FakeWallClock;
use nala::application::tools::Tool;
use nala::application::tools::current_time::CurrentTimeTool;

#[test]
fn reports_the_fixed_local_date_and_time() {
    let clock = FakeWallClock::new(2026, 8, 31, 14, 32, 0);
    let mut tool = CurrentTimeTool::new(clock);

    let result = tool.execute(()).expect("current_time should not fail");

    assert!(result.contains("14:32"));
    assert!(result.contains("31"));
    assert!(result.contains("2026"));
}

#[test]
fn ignores_any_arguments() {
    let result = CurrentTimeTool::<FakeWallClock>::parse_arguments("irrelevant");

    assert!(result.is_ok());
}

#[test]
fn has_no_context() {
    let clock = FakeWallClock::new(2026, 8, 31, 14, 32, 0);
    let mut tool = CurrentTimeTool::new(clock);

    let context = tool
        .context()
        .expect("current_time context should not fail");

    assert_eq!(context, "");
}
