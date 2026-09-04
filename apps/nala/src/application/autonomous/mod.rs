//! The autonomous event loop: an orchestration layer that turns external
//! events (device reports, timers, ...) into agent-loop turns, without
//! duplicating the agent loop itself. See `event_loop::AutonomousEventLoop`
//! for the runner, `policy::EventPolicy` for the ignore/act decision, and
//! `event::AutonomousEvent` for the event representation.

pub mod event;
pub mod event_loop;
pub mod policy;
