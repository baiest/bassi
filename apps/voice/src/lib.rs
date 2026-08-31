//! Voice: the interface layer. Turns Nala's events and answers into speech,
//! and (eventually) turns a microphone's audio into the text Nala receives.
//! Talks to Nala as a separate process over `client::NalaClient`, never as
//! a library — Voice depends on `agent-protocol` for the shared wire types,
//! never on `nala` itself.

pub mod bootstrap;
pub mod client;
pub mod narration;
pub mod narrator;
pub mod speaking_sink;
