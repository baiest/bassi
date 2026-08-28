pub struct ToolDefinition {
    pub name: &'static str,
    pub description: String,
    pub parameters: serde_json::Value,
}
