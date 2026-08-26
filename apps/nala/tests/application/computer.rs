use nala::application::computer::open_application;

use crate::fake_computer::FakeComputer;

#[test]
fn opens_application_when_requested() {
    let mut computer = FakeComputer::new();

    let result = open_application(&mut computer, "Chrome");

    assert!(result.is_ok());
    assert_eq!(computer.opened_application, Some("Chrome".to_string()))
}

#[test]
fn returns_error_when_application_cannot_be_oppened() {
    let mut computer = FakeComputer::new();
    computer.should_fail = true;

    let result = open_application(&mut computer, "FakeChrome");

    assert!(result.is_err())
}
