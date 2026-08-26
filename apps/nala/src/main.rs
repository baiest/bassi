use nala::adapters::computer::windows::Windows;
use nala::adapters::process::windows::Windows as WindowsProcess;
use nala::application::tools::Tool;
use nala::application::tools::execute_command::{ExecuteCommandArgs, ExecuteCommandTool};
use nala::application::tools::registry::ToolRegistry;

fn main() {
    let execute_command_definition = ExecuteCommandTool::<Windows<WindowsProcess>>::definition();

    let mut registry = ToolRegistry::new();

    registry.register(execute_command_definition);

    let tool_name = "execute_command";

    let definition = registry.get(tool_name).expect("Tool not found");

    println!("Found tool: {}", definition.name);

    let process = WindowsProcess::new();
    let computer = Windows::new(process);

    let mut tool = ExecuteCommandTool::new(computer);

    let args = ExecuteCommandArgs {
        command: "start chrome".to_string(),
    };

    tool.execute(args).expect("Tool execution failed");
}
