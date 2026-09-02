use super::ToolDefinition;

/// Static tools (natives + MCP) plus a device layer that's replaced wholesale
/// once per turn (see `Assistant::process`). Static tools are kept in
/// registration order, not a `HashMap`, so `definitions()` returns the same
/// sequence on every call — the tool list is serialized into the LLM prompt
/// on every turn, and a `HashMap`'s randomized iteration order would
/// reshuffle it each time, breaking the backend's prompt-prefix cache (e.g.
/// Ollama/llama.cpp). A device tool shadows a static one of the same bare
/// name — e.g. a connected device publishing `pc_open_url` hides the native
/// `open_url` — so the model is only ever offered one way to do a given
/// action, and it's the one that actually reaches the device the user meant.
pub struct ToolRegistry {
    tools: Vec<ToolDefinition>,
    device_tools: Vec<ToolDefinition>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: Vec::new(),
            device_tools: Vec::new(),
        }
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
        self.tools
            .iter()
            .find(|tool| tool.name == name)
            .filter(|_| !self.shadowed(name))
            .or_else(|| self.device_tools.iter().find(|tool| tool.name == name))
    }

    /// Replaces the device layer wholesale — called once per turn with the
    /// currently-connected devices' tools, so a device that connects or
    /// disconnects mid-session is reflected on the next turn without
    /// needing the turn-client to reconnect.
    pub fn set_device_tools(&mut self, device_tools: Vec<ToolDefinition>) {
        self.device_tools = device_tools;
    }

    /// A device tool's bare capability name is its name after its
    /// `<device>_` prefix — e.g. `pc_open_url` shadows the static `open_url`.
    fn shadowed(&self, static_name: &str) -> bool {
        self.device_tools
            .iter()
            .any(|tool| bare_capability(&tool.name) == static_name)
    }

    pub fn definitions(&self) -> Vec<&ToolDefinition> {
        let statics = self
            .tools
            .iter()
            .filter(|definition| !self.shadowed(&definition.name));
        self.device_tools.iter().chain(statics).collect()
    }
}

/// Strips a device tool's `<device>_` prefix, e.g. `pc_open_url` -> `open_url`.
fn bare_capability(device_tool_name: &str) -> &str {
    device_tool_name
        .split_once('_')
        .map(|(_, capability)| capability)
        .unwrap_or(device_tool_name)
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition(name: &str) -> ToolDefinition {
        ToolDefinition {
            name: name.to_string(),
            description: String::new(),
            parameters: serde_json::json!({}),
        }
    }

    #[test]
    fn a_device_tool_hides_the_static_tool_it_shadows() {
        let mut registry = ToolRegistry::new();
        registry.register(definition("open_url"));
        registry.set_device_tools(vec![definition("pc_open_url")]);

        let names: Vec<&str> = registry
            .definitions()
            .iter()
            .map(|definition| definition.name.as_str())
            .collect();

        assert_eq!(names, vec!["pc_open_url"]);
    }

    #[test]
    fn a_static_tool_with_no_shadowing_device_tool_survives() {
        let mut registry = ToolRegistry::new();
        registry.register(definition("execute_command"));
        registry.set_device_tools(vec![definition("pc_open_url")]);

        let names: Vec<&str> = registry
            .definitions()
            .iter()
            .map(|definition| definition.name.as_str())
            .collect();

        assert_eq!(names, vec!["pc_open_url", "execute_command"]);
    }

    #[test]
    fn clearing_the_device_layer_restores_the_static_tool() {
        let mut registry = ToolRegistry::new();
        registry.register(definition("open_url"));
        registry.set_device_tools(vec![definition("pc_open_url")]);

        registry.set_device_tools(Vec::new());

        let names: Vec<&str> = registry
            .definitions()
            .iter()
            .map(|definition| definition.name.as_str())
            .collect();

        assert_eq!(names, vec!["open_url"]);
    }

    #[test]
    fn get_returns_the_device_tool_when_shadowing_a_static_one() {
        let mut registry = ToolRegistry::new();
        registry.register(definition("open_url"));
        registry.set_device_tools(vec![definition("pc_open_url")]);

        assert!(registry.get("open_url").is_none());
        assert_eq!(registry.get("pc_open_url").unwrap().name, "pc_open_url");
    }
}
