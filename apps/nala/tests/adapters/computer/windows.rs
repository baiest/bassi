use std::time::Duration;

use nala::adapters::computer::windows::Windows;
use nala::ports::computer::Computer;

use crate::fake_environment::FakeEnvironment;
use crate::fake_process::FakeProcess;

#[test]
fn opens_application() {
    let process = FakeProcess::new();
    let environment = FakeEnvironment::new();
    let mut computer = Windows::new(process, environment);

    let result = computer.execute_command("Chrome", Duration::from_secs(30));

    assert!(result.is_ok())
}

#[test]
fn builds_context_from_the_environment() {
    let process = FakeProcess::new();
    let environment = FakeEnvironment::new()
        .with_var("USERNAME", "juan")
        .with_var("USERPROFILE", r"C:\Users\juan")
        .with_current_dir(r"C:\Users\juan\projects\bassi");
    let mut computer = Windows::new(process, environment);

    let context = computer.get_context().expect("context should build");

    assert_eq!(context.username, "juan");
    assert_eq!(context.home_dir, r"C:\Users\juan");
    assert_eq!(context.desktop_dir, r"C:\Users\juan\Desktop");
    assert_eq!(context.current_dir, r"C:\Users\juan\projects\bassi");
}

#[test]
fn fails_to_build_context_when_the_environment_fails() {
    let process = FakeProcess::new();
    let mut environment = FakeEnvironment::new();
    environment.should_fail = true;
    let mut computer = Windows::new(process, environment);

    let result = computer.get_context();

    assert!(result.is_err());
}
