use device_protocol::{CapabilityDefinition, Outcome};

/// A device (e.g. the PC daemon) connected to Nala over the device
/// protocol, from the agent loop's point of view. `invoke` is infallible —
/// a disconnected device, a timeout, or a capability failure all come back
/// as `Outcome::Err` rather than an `Err` return, since the agent loop
/// always needs *something* to hand back to the model, never a hang or a
/// panic. The real implementation (a WebSocket connection, added once the
/// device server exists) is responsible for turning transport failures
/// into the right `Outcome::Err` variant.
pub trait RemoteDevice {
    /// The name this device registered under (e.g. `"pc"`), used as the
    /// prefix on every capability it publishes as a tool.
    fn name(&self) -> &str;

    fn capabilities(&self) -> &[CapabilityDefinition];

    fn invoke(&mut self, capability: &str, arguments: &str) -> Outcome;
}
