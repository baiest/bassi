use crate::ports::events::{Event, EventSink, TurnState};

pub struct ConsoleEventSink;

impl EventSink for ConsoleEventSink {
    fn emit(&mut self, event: crate::ports::events::Event) {
        match event {
            Event::RequestStarted => {
                println!("[REQUEST] started");
            }
            Event::StateChanged { state } => {
                let label = match state {
                    TurnState::Receiving => "receiving",
                    TurnState::Planning => "planning",
                    TurnState::Thinking => "thinking",
                    TurnState::Executing => "executing",
                    TurnState::Verifying => "verifying",
                    TurnState::Responding => "responding",
                };
                println!("[STATE] {label}");
            }
            Event::RequestCompleted { duration } => {
                println!("[REQUEST] completed in {:?}\n", duration);
            }

            Event::RequestFailed { duration, error } => {
                println!("[REQUEST] failed in {:?}: {}\n", duration, error);
            }
            Event::PlanCreated { plan } => {
                println!("[PLAN]\n{plan}\n");
            }
            Event::LlmStarted { images } => {
                if images > 0 {
                    println!("[LLM] started (with {images} image(s) attached)");
                } else {
                    println!("[LLM] started");
                }
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
                images,
            } => {
                let images_note = if images > 0 {
                    format!(" ({images} image(s))")
                } else {
                    String::new()
                };
                println!(
                    "[TOOL] [{name}] completed in {:?}{images_note}: {:?}\n",
                    duration, output
                )
            }
            Event::Retrying { attempt, error } => {
                println!("[RETRY] attempt {attempt} after error: {error}");
            }
            Event::Cancelled => {
                println!("[CANCELLED] turn stopped\n");
            }
        }
    }
}
