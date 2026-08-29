#[path = "common/fake_computer.rs"]
mod fake_computer;

#[path = "common/fake_llm.rs"]
mod fake_llm;

#[path = "common/fake_mcp.rs"]
mod fake_mcp;

#[path = "common/fake_events.rs"]
mod fake_events;

#[path = "common/fake_clock.rs"]
mod fake_clock;

#[path = "common/fake_cancel.rs"]
mod fake_cancel;

#[path = "application/tools/execute_command.rs"]
mod execute_command;

#[path = "application/tools/registry.rs"]
mod registry;

#[path = "application/tools/ping.rs"]
mod ping;

#[path = "application/tools/dispatcher.rs"]
mod dispatcher;

#[path = "application/tools/computer_use.rs"]
mod computer_use;

#[path = "application/assistant.rs"]
mod assistant;

#[path = "application/context_budget.rs"]
mod context_budget;
