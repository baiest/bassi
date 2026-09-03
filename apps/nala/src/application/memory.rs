use std::collections::BTreeMap;

/// A small set of durable, user-taught facts (e.g. `nombre` -> `Juan`),
/// separate from the conversation transcript so they survive both
/// per-turn eviction (`context_budget.rs`) and process restarts.
///
/// Pure parse/format/upsert logic only — no filesystem access. See
/// `adapters::memory` for the I/O around it, same split as
/// `nala-overlay`'s `config.rs`.
///
/// Backed by a `BTreeMap` (not the insertion-ordered `HashMap` mistake
/// fixed for `ToolRegistry`) so `to_json()` is byte-identical regardless of
/// the order facts were remembered in — the serialized form feeds directly
/// into a system message re-injected every turn, and its shape must not
/// vary for reasons unrelated to what's actually remembered.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facts(BTreeMap<String, String>);

impl Facts {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.0.iter()
    }

    /// Records `key` = `value`, replacing any existing value for `key`.
    pub fn remember(&mut self, key: String, value: String) {
        self.0.insert(key, value);
    }

    /// Parses a JSON object of `{key: value}` pairs. Anything that isn't
    /// valid JSON, or isn't an object of strings, yields an empty store
    /// rather than an error — a missing, corrupt, or hand-edited memory
    /// file should never stop the assistant from starting, it should just
    /// start with no memory, the same as a first run.
    pub fn parse(json: &str) -> Self {
        if json.trim().is_empty() {
            return Self::new();
        }

        serde_json::from_str(json).map(Self).unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(&self.0).unwrap_or_else(|_| "{}".to_string())
    }
}
