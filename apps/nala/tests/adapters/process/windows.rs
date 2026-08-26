use nala::adapters::process::windows::Windows;
use nala::ports::process::Process;

#[test]
fn spawns_process() {
    let mut process = Windows::new();

    let result = process.spawn("cmd.exe", &[]);

    assert!(result.is_ok());
}
