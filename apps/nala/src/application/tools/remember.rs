use schemars::JsonSchema;
use serde::Deserialize;

use crate::application::tools::Tool;
use crate::ports::memory::{MemoryError, MemoryStore};

#[derive(Deserialize, JsonSchema)]
pub struct RememberArgs {
    /// A short name for the fact, e.g. "nombre" or "ciudad".
    pub key: String,
    /// The value to remember for that key.
    pub value: String,
}

/// Lets the model persist a durable fact the user shared (their name, a
/// preference, ...) outside the conversation transcript, so it survives
/// both per-turn history eviction and process restarts. Remembering an
/// already-known key overwrites its value instead of accumulating a
/// contradictory second entry — see `application::memory::Facts::remember`.
///
/// Owns its `MemoryStore` as a trait object rather than a generic
/// parameter so `Tools` (in `dispatcher.rs`) doesn't need a 6th generic
/// parameter just for this one tool.
pub struct RememberTool {
    store: Box<dyn MemoryStore>,
}

impl RememberTool {
    pub fn new(store: Box<dyn MemoryStore>) -> Self {
        Self { store }
    }

    /// All facts currently in the underlying store — exposed for tests, the
    /// same way `ExecuteCommandTool::context()` lets a test observe state
    /// the tool changed.
    pub fn facts(&mut self) -> Vec<(String, String)> {
        self.store.facts()
    }
}

impl Tool for RememberTool {
    type Args = RememberArgs;
    type Output = String;
    type Error = MemoryError;

    const NAME: &'static str = "remember";
    const DESCRIPTION: &'static str = "Remember a durable fact about the user (e.g. their name, a preference, a location) for future conversations. Remembering the same key again replaces its previous value.";
    const MUTATING: bool = true;

    fn parameters() -> serde_json::Value {
        serde_json::to_value(schemars::schema_for!(RememberArgs))
            .expect("RememberArgs schema should serialize to JSON")
    }

    fn execute(&mut self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.store.remember(args.key.clone(), args.value.clone())?;
        Ok(format!("Recordado: {} = {}", args.key, args.value))
    }

    fn parse_arguments(arguments: &str) -> Result<Self::Args, Self::Error> {
        serde_json::from_str(arguments)
            .map_err(|error| MemoryError::InvalidArguments(error.to_string()))
    }

    fn context(&mut self) -> Result<String, Self::Error> {
        Ok(String::new())
    }
}
