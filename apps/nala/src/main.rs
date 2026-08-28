use std::io::{self, Write};

use nala::adapters::computer::windows::Windows;
use nala::adapters::llm::ollama::OllamaLlm;
use nala::adapters::process::windows::Windows as WindowsProcess;
use nala::application::assistant::Assistant;
use nala::application::tools::Tool;
use nala::application::tools::dispatcher::ToolDispatcher;
use nala::application::tools::execute_command::ExecuteCommandTool;
use nala::application::tools::registry::ToolRegistry;

fn main() {
    let process = WindowsProcess::new();

    let computer = Windows::new(process);

    let mut registry = ToolRegistry::new();
    registry.register(ExecuteCommandTool::<Windows<WindowsProcess>>::definition());

    let tool = ExecuteCommandTool::new(computer);

    let mut dispatcher = ToolDispatcher::new();

    dispatcher.register(tool);

    let llm: OllamaLlm = OllamaLlm::new("http://localhost:11434", "qwen3.5:2b")
        .expect("Failed to create Ollama client");

    let mut assistant = Assistant::new(llm, dispatcher, registry);

    let mut input = String::new();
    loop {
        println!("Hola, en que te puedo ayudar?");
        print!(">");

        io::stdout().flush().expect("Error cleaning buffer");

        io::stdin()
            .read_line(&mut input)
            .expect("Failed reading line");

        println!("Procesando tu petición...");
        let result = assistant.process(input.trim());

        match result {
            Ok(_) => {
                println!("Respuesta recibida con éxito.");
            }
            Err(e) => {
                eprintln!("Error: No se pudo procesar la petición. Detalles: {:?}", e);
            }
        }

        input.clear();
    }
}
