//! Nala's event types and the port that consumes them live in
//! `agent-protocol`, shared with any client (e.g. `voice`) that talks to
//! Nala over a connection — re-exported here so existing call sites inside
//! this crate don't need to change.

pub use agent_protocol::{
    BudgetStep, Event, EventSink, LlmCallId, RequestSource, TaskId, TurnState,
};
