use std::io::{self, Write};

use nala::adapters::computer::windows::Windows;
use nala::adapters::environment::system::SystemEnvironment;
use nala::adapters::events::console::ConsoleEventSink;
use nala::adapters::llm::ollama::OllamaLlm;
use nala::adapters::mcp::child_process::ChildTransport;
use nala::adapters::mcp::stdio::StdioMcpClient;
use nala::adapters::process::windows::Windows as WindowsProcess;
use nala::application::assistant::Assistant;
use nala::application::tools::Tool;
use nala::application::tools::computer_use::{ComputerUseToolset, DEFAULT_ALLOWLIST};
use nala::application::tools::dispatcher::{ToolDispatcher, Tools};
use nala::application::tools::execute_command::ExecuteCommandTool;
use nala::application::tools::ping::PingTool;
use nala::application::tools::registry::ToolRegistry;

type ComputerType = Windows<WindowsProcess, SystemEnvironment>;
type McpClientType = StdioMcpClient<ChildTransport>;

fn main() {
    let process = WindowsProcess::new();
    let environment = SystemEnvironment::new();

    let computer = Windows::new(process, environment);

    let mut registry = ToolRegistry::new();
    registry.register(ExecuteCommandTool::<ComputerType>::definition());
    registry.register(PingTool::definition());

    let mut dispatcher: ToolDispatcher<ComputerType, McpClientType> = ToolDispatcher::new();

    dispatcher.register(Tools::ExecuteCommand(ExecuteCommandTool::new(computer)));
    dispatcher.register(Tools::Ping(PingTool::new()));

    // Desktop control (screenshot, click, type, ...) via computer-use-mcp,
    // spawned over stdio and exposed to the model as a filtered set of
    // tools. This block is the only place computer-use-mcp is referenced;
    // deleting it (or swapping the spawned command) is all it takes to
    // remove or replace the integration. Set NALA_MCP=off to run without it
    // (e.g. when Node/npx isn't available).
    if std::env::var("NALA_MCP").as_deref() != Ok("off") {
        match connect_computer_use() {
            Ok(toolset) => {
                for definition in toolset.definitions() {
                    registry.register(definition.clone());
                }
                dispatcher.register(Tools::ComputerUse(toolset));
            }
            Err(error) => {
                eprintln!(
                    "Warning: could not start computer-use-mcp ({error}); \
                     Nala will run without desktop control tools."
                );
            }
        }
    }

    let llm: OllamaLlm = OllamaLlm::new("http://localhost:11434", "gemma4:e4b")
        .expect("Failed to create Ollama client");

    let events = ConsoleEventSink;

    let mut assistant = Assistant::new(llm, dispatcher, registry, events);

    let mut input = String::new();
    loop {
        println!("Hola, en que te puedo ayudar?");
        print!("> ");

        io::stdout().flush().expect("Error cleaning buffer");

        io::stdin()
            .read_line(&mut input)
            .expect("Failed reading line");

        match assistant.process(input.trim()) {
            Ok(response) => println!("{response}"),
            Err(e) => eprintln!("Error: {e}"),
        }

        input.clear();
    }
}

fn connect_computer_use() -> Result<ComputerUseToolset<McpClientType>, String> {
    let transport = ChildTransport::spawn("npx", &["-y", "@zavora-ai/computer-use-mcp@7.1.0"])
        .map_err(|error| error.to_string())?;

    let client = StdioMcpClient::new(transport);

    ComputerUseToolset::connect(client, DEFAULT_ALLOWLIST).map_err(|error| error.to_string())
}
