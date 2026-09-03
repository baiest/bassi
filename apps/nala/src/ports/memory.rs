/// A store of durable facts the assistant has been taught, outside the
/// conversation transcript — see `application::memory::Facts` for the pure
/// data it holds. Kept separate from any specific backend so tests can
/// substitute an in-memory store instead of touching the filesystem.
pub trait MemoryStore {
    /// All remembered facts, as `(key, value)` pairs.
    fn facts(&mut self) -> Vec<(String, String)>;

    /// Records `key` = `value`, replacing any existing value for `key`.
    fn remember(&mut self, key: String, value: String) -> Result<(), MemoryError>;
}

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("could not save memory: {0}")]
    Save(String),
    #[error("invalid remember arguments: {0}")]
    InvalidArguments(String),
}
