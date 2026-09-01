//! Device capabilities: the OS-facing ports/adapters and the concrete
//! implementations (`execute_command`, `open_app`, `open_url`, `volume`,
//! `list_apps`) that a device daemon (e.g. the PC daemon) runs. Extracted
//! out of `nala` so both `nala` (for same-machine use) and a daemon binary
//! can depend on the same code instead of duplicating it.

pub mod adapters;
pub mod capabilities;
pub mod capability;
pub mod ports;
pub mod registry;

pub use capability::Capability;
