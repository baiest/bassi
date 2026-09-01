/// What the PC voice companion is doing right now, driving both the
/// overlay's color and (via `crate::amplitude`) how it reacts to sound.
/// Local to this app — unlike the old `pc-daemon`-relayed design, nothing
/// here comes from Nala or any other process; the app always knows its own
/// status directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Idle,
    /// Recording the mic (the circle reacts to live input level).
    Listening,
    /// The recorded utterance is uploading to `voice --serve`.
    Sending,
    /// Playing back a clip from `voice --serve` (the circle reacts to that
    /// clip's real amplitude).
    Speaking,
    Error,
}
