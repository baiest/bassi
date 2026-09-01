use device_protocol::Outcome;

use crate::application::tools::ToolDefinition;
use crate::ports::device::RemoteDevice;
use crate::ports::tool_dispatcher::ToolOutcome;

/// Publishes one connected `RemoteDevice`'s capabilities as tools, prefixed
/// with the device's name (`pc_open_app`, not `open_app`) — same shape as
/// `McpToolset`, but for a device instead of an MCP server. The prefix
/// keeps a device's capabilities from colliding with Nala's own native
/// tools or another device's, and tells the model *where* an action
/// happens ("open Spotify on my PC").
pub struct DeviceToolset<D: RemoteDevice> {
    device: D,
}

impl<D: RemoteDevice> DeviceToolset<D> {
    pub fn new(device: D) -> Self {
        Self { device }
    }

    fn prefix(&self) -> String {
        format!("{}_", self.device.name())
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        let prefix = self.prefix();
        self.device
            .capabilities()
            .iter()
            .map(|capability| ToolDefinition {
                name: format!("{prefix}{}", capability.name),
                description: capability.description.clone(),
                parameters: capability.parameters.clone(),
            })
            .collect()
    }

    pub fn handles(&self, name: &str) -> bool {
        self.definitions()
            .iter()
            .any(|definition| definition.name == name)
    }

    /// Exposed for tests, to inspect what was sent to the device.
    pub fn device(&self) -> &D {
        &self.device
    }

    /// Infallible: whatever `RemoteDevice::invoke` reports — success,
    /// failure, or a transport problem turned into `Outcome::Err` by the
    /// device implementation — always becomes a `ToolOutcome` the agent
    /// loop can hand back to the model, never a dispatcher error.
    pub fn call(&mut self, name: &str, arguments: &str) -> ToolOutcome {
        let prefix = self.prefix();
        let capability = name.strip_prefix(&prefix).unwrap_or(name);

        match self.device.invoke(capability, arguments) {
            Outcome::Ok { text, mutated } => ToolOutcome {
                text,
                images: Vec::new(),
                mutated,
            },
            Outcome::Err { code, message } => {
                ToolOutcome::from(format!("ERROR ({code:?}): {message}"))
            }
        }
    }
}
