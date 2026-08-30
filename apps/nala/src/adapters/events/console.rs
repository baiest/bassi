use crate::ports::events::{BudgetStep, Event, EventSink, TurnState};

pub struct ConsoleEventSink;

impl EventSink for ConsoleEventSink {
    fn emit(&mut self, event: crate::ports::events::Event) {
        match event {
            Event::RequestStarted { .. } => {
                println!("[REQUEST] started");
            }
            Event::StateChanged { state, .. } => {
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
            Event::RequestCompleted { duration, .. } => {
                println!("[REQUEST] completed in {:?}\n", duration);
            }

            Event::RequestFailed {
                duration, error, ..
            } => {
                println!("[REQUEST] failed in {:?}: {}\n", duration, error);
            }
            Event::PlanCreated { plan, .. } => {
                println!("[PLAN]\n{plan}\n");
            }
            Event::LlmStarted {
                call_index, images, ..
            } => {
                if images > 0 {
                    println!("[LLM] call #{call_index} started (with {images} image(s) attached)");
                } else {
                    println!("[LLM] call #{call_index} started");
                }
            }
            Event::LlmCompleted {
                call_index,
                duration,
                ..
            } => {
                println!("[LLM] call #{call_index} completed in {:?}\n", duration);
            }
            Event::LlmFailed {
                call_index,
                duration,
                error,
                ..
            } => {
                println!(
                    "[LLM] call #{call_index} failed in {:?}: {}\n",
                    duration, error
                );
            }
            Event::ToolStarted {
                tool_call_index,
                name,
                arguments,
                ..
            } => {
                println!("[TOOL] #{tool_call_index} [{name}] started with arguments: {arguments}")
            }
            Event::ToolCompleted {
                tool_call_index,
                name,
                duration,
                output,
                images,
                ..
            } => {
                let images_note = if images > 0 {
                    format!(" ({images} image(s))")
                } else {
                    String::new()
                };
                println!(
                    "[TOOL] #{tool_call_index} [{name}] completed in {:?}{images_note}: {:?}\n",
                    duration, output
                )
            }
            Event::Retrying { attempt, error, .. } => {
                println!("[RETRY] attempt {attempt} after error: {error}");
            }
            Event::Cancelled { .. } => {
                println!("[CANCELLED] turn stopped\n");
            }
            Event::TokensUsed {
                call_index,
                prompt_tokens,
                completion_tokens,
                ..
            } => {
                let prompt = prompt_tokens
                    .map(|tokens| tokens.to_string())
                    .unwrap_or_else(|| "?".to_string());
                let completion = completion_tokens
                    .map(|tokens| tokens.to_string())
                    .unwrap_or_else(|| "?".to_string());
                println!("[TOKENS] call #{call_index} prompt={prompt} completion={completion}");
            }
            Event::BudgetPressure {
                step,
                remaining_estimate,
                ..
            } => {
                let label = match step {
                    BudgetStep::DroppedImages { count } => format!("dropped {count} image(s)"),
                    BudgetStep::TruncatedText { count } => {
                        format!("truncated {count} tool result(s)")
                    }
                    BudgetStep::DroppedTurns { count } => format!("dropped {count} old turn(s)"),
                };
                println!("[BUDGET] {label}, ~{remaining_estimate} tokens remaining");
            }
            Event::TranscriptCompacted {
                turns_compacted, ..
            } => {
                println!("[BUDGET] compacted {turns_compacted} old turn(s) into a summary\n");
            }
            Event::AnsweredUnverified { .. } => {
                println!("[VERIFY] answered without checking the last action\n");
            }
        }
    }
}
