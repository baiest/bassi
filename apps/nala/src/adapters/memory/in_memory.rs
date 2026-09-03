use crate::application::memory::Facts;
use crate::ports::memory::{MemoryError, MemoryStore};

/// A `MemoryStore` that never touches disk — `Assistant::new`'s default, so
/// a caller that doesn't opt into persistence (most tests, and any
/// `Assistant` built without `with_memory`) doesn't need a real file. Also
/// handy as a test fixture for seeding facts without filesystem I/O.
#[derive(Debug, Clone, Default)]
pub struct InMemoryMemoryStore {
    facts: Facts,
}

impl InMemoryMemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl MemoryStore for InMemoryMemoryStore {
    fn facts(&mut self) -> Vec<(String, String)> {
        self.facts
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }

    fn remember(&mut self, key: String, value: String) -> Result<(), MemoryError> {
        self.facts.remember(key, value);
        Ok(())
    }
}
