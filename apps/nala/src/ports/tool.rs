#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// `device_capabilities::Capability::definition()` returns a
/// `device_protocol::CapabilityDefinition` rather than this type — the
/// capabilities crate can't depend on `nala`'s types without a dependency
/// cycle, so the two are structurally identical and this is a straight
/// field copy.
impl From<device_protocol::CapabilityDefinition> for ToolDefinition {
    fn from(capability: device_protocol::CapabilityDefinition) -> Self {
        Self {
            name: capability.name,
            description: capability.description,
            parameters: capability.parameters,
        }
    }
}
