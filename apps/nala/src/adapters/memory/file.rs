use std::path::PathBuf;

use crate::application::memory::Facts;
use crate::ports::memory::{MemoryError, MemoryStore};

/// Persists facts as a JSON object at `path`, reloaded once at construction
/// and rewritten in full on every `remember`. A missing or corrupt file is
/// never a fatal error — it just means starting with no memory, the same
/// as a first run (see `Facts::parse`).
pub struct FileMemoryStore {
    path: PathBuf,
    facts: Facts,
}

impl FileMemoryStore {
    pub fn new(path: PathBuf) -> Self {
        let facts = std::fs::read_to_string(&path)
            .map(|text| Facts::parse(&text))
            .unwrap_or_default();

        Self { path, facts }
    }
}

impl MemoryStore for FileMemoryStore {
    fn facts(&mut self) -> Vec<(String, String)> {
        self.facts
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }

    fn remember(&mut self, key: String, value: String) -> Result<(), MemoryError> {
        self.facts.remember(key, value);

        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|error| MemoryError::Save(error.to_string()))?;
        }

        std::fs::write(&self.path, self.facts.to_json())
            .map_err(|error| MemoryError::Save(error.to_string()))
    }
}
