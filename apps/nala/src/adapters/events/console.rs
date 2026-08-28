use crate::ports::events::{Event, EventSink};

pub struct ConsoleEventSink;

impl EventSink for ConsoleEventSink {
    fn emit(&mut self, event: crate::ports::events::Event) {
        match event {
            Event::RequestStarted => {
                println!("[REQUEST] started");
            }
            Event::RequestCompleted { duration } => {
                println!("[REQUEST] completed in {:?}\n", duration);
            }

            Event::RequestFailed { duration, error } => {
                println!("[REQUEST] failed in {:?}: {}\n", duration, error);
            }
            Event::LlmStarted => {
                println!("[LLM] started");
            }
            Event::LlmCompleted { duration } => {
                println!("[LLM] completed in {:?}\n", duration);
            }
            Event::ToolStarted { name, arguments } => {
                println!("[TOOL] [{name}] started with arguments: {arguments}")
            }
            Event::ToolCompleted {
                name,
                duration,
                output,
            } => {
                println!(
                    "[TOOL] [{name}] completed in {:?}: {:?}\n",
                    duration, output
                )
            }
        }
    }
}
