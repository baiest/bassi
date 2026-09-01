//! A small floating overlay: subscribes to the PC daemon's overlay channel
//! and renders a circle whose color follows the daemon's current
//! `DeviceState`. Never talks to Nala directly, never runs a capability —
//! purely a viewer over what the daemon already broadcasts.

pub mod color;
