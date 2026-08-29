use crate::ports::events::Event;

/// Decides what Nala should say out loud in reaction to a turn event — pure
/// text, no audio. Kept separate from `Speech` so the phrase-selection
/// policy (canned templates today, an LLM-generated filler tomorrow) can
/// change without touching how or whether it gets spoken.
pub trait Narrator {
    /// The phrase to say for this event, or `None` if it doesn't warrant
    /// narration. `&mut self` so an implementation can track state (phrase
    /// rotation, suppressing an immediate repeat).
    fn narrate(&mut self, event: &Event) -> Option<String>;
}
