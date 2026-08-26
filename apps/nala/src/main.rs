use nala::adapters::computer::windows::Windows;
use nala::adapters::process::windows::Windows as WindowsProcess;
use nala::ports::computer::Computer;

fn main() {
    let process = WindowsProcess::new();
    let mut computer = Windows::new(process);

    computer
        .execute_command("spotify")
        .expect("Failed to open chrome")
}
