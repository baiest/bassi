use nala::adapters::computer::windows::Windows;
use nala::adapters::process::windows::Windows as WindowsProcess;
use nala::application::tools::Tool;
use nala::application::tools::execute_command::{ExecuteCommandArgs, ExecuteCommandTool};

fn main() {
    let process = WindowsProcess::new();
    let computer = Windows::new(process);
    let mut tool = ExecuteCommandTool::new(computer);
    let args = ExecuteCommandArgs {
        command: "start chrome".to_string(),
    };

    tool.execute(args).expect("Failed to open chrome")
}
