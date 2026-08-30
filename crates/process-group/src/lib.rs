//! A Windows Job Object wrapper shared by every adapter that spawns a child
//! process and needs it (and anything that child later spawns) killed
//! together. See [`ProcessGroup`] for why this matters.

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::ProcessGroup;
