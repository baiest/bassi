use device_protocol::{CapabilityDefinition, Outcome};
use nala::ports::device::RemoteDevice;

/// A `RemoteDevice` whose capability list and `invoke` response are both
/// scripted, and which records the last capability/arguments it was asked
/// to invoke — enough to test `DeviceToolset` and the dispatcher's routing
/// without a real connection.
pub struct FakeDevice {
    name: String,
    capabilities: Vec<CapabilityDefinition>,
    outcome: Outcome,
    pub last_invoke: Option<(String, String)>,
}

impl FakeDevice {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            capabilities: Vec::new(),
            outcome: Outcome::Ok {
                text: String::new(),
                mutated: false,
            },
            last_invoke: None,
        }
    }

    pub fn with_capability(mut self, name: &str, description: &str) -> Self {
        self.capabilities.push(CapabilityDefinition {
            name: name.to_string(),
            description: description.to_string(),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
        });
        self
    }

    pub fn returning(mut self, outcome: Outcome) -> Self {
        self.outcome = outcome;
        self
    }
}

impl RemoteDevice for FakeDevice {
    fn name(&self) -> &str {
        &self.name
    }

    fn capabilities(&self) -> &[CapabilityDefinition] {
        &self.capabilities
    }

    fn invoke(&mut self, capability: &str, arguments: &str) -> Outcome {
        self.last_invoke = Some((capability.to_string(), arguments.to_string()));
        self.outcome.clone()
    }
}
