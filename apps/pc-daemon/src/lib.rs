//! The PC device daemon: connects to Nala, announces the capabilities it
//! runs, and executes what Nala invokes. Never runs an LLM or an agent
//! loop — decision-making stays on Nala's side; this crate only carries
//! out `Invoke` requests via `device-capabilities` and reports back.

pub mod client;
pub mod config;
pub mod daemon;
pub mod reconnect;
