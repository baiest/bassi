//! A small floating overlay that's also a PC voice client: connects
//! directly to `voice --serve` (the same protocol `apps/android` speaks),
//! records the mic, sends utterances, plays back what comes back, and
//! renders a circle that reacts to the real amplitude of whatever it's
//! recording or playing. Never talks to Nala or `pc-daemon` — Nala never
//! sees audio, and this app has no capability-execution concern.

pub mod amplitude;
pub mod clip;
pub mod color;
pub mod config;
pub mod motion;
pub mod scene;
pub mod status;

#[cfg(windows)]
pub mod playback;
#[cfg(windows)]
pub mod voice_client;
