use nala::adapters::computer::windows::Windows;
use nala::ports::computer::Computer;

use crate::fake_process::FakeProcess;

#[test]
fn opens_application() {
    let process = FakeProcess::new();
    let mut computer = Windows::new(process);

    let result = computer.open_application("Chrome");

    assert!(result.is_ok())
}
