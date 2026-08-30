//! Voice: the interface layer. Turns Nala's events and answers into speech,
//! and (eventually) turns a microphone's audio into the text Nala receives.
//! Depends on `tts` for the speech engine and on `nala` for the `Event`
//! types it narrates — never the other way around.

pub mod bootstrap;
pub mod narration;
pub mod narrator;
pub mod speaking_sink;
