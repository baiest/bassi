use std::sync::{Arc, Mutex};

use device_protocol::{CapabilityDefinition, DeviceState, Outcome};
use nala::ports::device::RemoteDevice;

/// A `RemoteDevice` whose capability list and `invoke` response are both
/// scripted, and which records the last capability/arguments it was asked
/// to invoke, and every `DeviceState` it was ever pushed — enough to test
/// `DeviceToolset`, the dispatcher's routing, and state broadcasting
/// without a real connection. `Clone`-able (state shared via `Arc`, like
/// the real `WsDevice`) so it works with `DeviceRegistry`, which hands out
/// clones rather than references.
#[derive(Clone)]
pub struct FakeDevice {
    name: String,
    capabilities: Vec<CapabilityDefinition>,
    outcome: Outcome,
    pub last_invoke: Option<(String, String)>,
    pushed_states: Arc<Mutex<Vec<DeviceState>>>,
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
            pushed_states: Arc::new(Mutex::new(Vec::new())),
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

    pub fn pushed_states(&self) -> Vec<DeviceState> {
        self.pushed_states.lock().unwrap().clone()
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

    fn push_state(&self, state: DeviceState) {
        self.pushed_states.lock().unwrap().push(state);
    }
}
