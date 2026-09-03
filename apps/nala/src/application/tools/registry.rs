use super::ToolDefinition;

/// Tools are kept in registration order, not a `HashMap`, so `definitions()`
/// returns the exact same sequence on every call. The tool list is
/// serialized into the LLM prompt on every turn — a `HashMap`'s randomized
/// iteration order would reshuffle it each time, breaking the backend's
/// prompt-prefix cache (e.g. Ollama/llama.cpp) and forcing it to reprocess
/// the whole tool schema from scratch instead of reusing what it already
/// cached.
pub struct ToolRegistry {
    tools: Vec<ToolDefinition>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    pub fn register(&mut self, definition: ToolDefinition) {
        match self
            .tools
            .iter_mut()
            .find(|tool| tool.name == definition.name)
        {
            Some(existing) => *existing = definition,
            None => self.tools.push(definition),
        }
    }

    pub fn get(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.iter().find(|tool| tool.name == name)
    }

    pub fn definitions(&self) -> Vec<&ToolDefinition> {
        self.tools.iter().collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
