use std::collections::HashMap;
use std::sync::Mutex;

/// Every device currently connected to Nala, keyed by `device_id`. Shared
/// between the device server's accept loop (which registers/removes
/// devices as connections come and go) and `bootstrap`, which snapshots it
/// once per turn-client connection to build that connection's
/// `Tools::Devices`.
///
/// A snapshot is a point-in-time copy: a device connecting or disconnecting
/// mid-turn-client-connection isn't picked up until that connection is
/// re-established, the same limitation Nala's MCP tools already have (MCP
/// servers are connected once at startup, not rediscovered per turn). This
/// keeps the design simple; live tool-list updates are future work, not
/// needed for the vertical slice.
pub struct DeviceRegistry<D: Clone> {
    devices: Mutex<HashMap<String, D>>,
}

impl<D: Clone> DeviceRegistry<D> {
    pub fn new() -> Self {
        Self {
            devices: Mutex::new(HashMap::new()),
        }
    }

    /// Registers `device` under `device_id`, replacing whatever was there —
    /// a device reconnecting with the same id (its daemon restarted, its
    /// network blipped) takes over the slot instead of piling up alongside
    /// the stale entry.
    pub fn register(&self, device_id: String, device: D) {
        self.devices.lock().unwrap().insert(device_id, device);
    }

    pub fn remove(&self, device_id: &str) {
        self.devices.lock().unwrap().remove(device_id);
    }

    pub fn snapshot(&self) -> Vec<D> {
        self.devices.lock().unwrap().values().cloned().collect()
    }
}

impl<D: Clone> Default for DeviceRegistry<D> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_snapshot_includes_every_registered_device() {
        let registry: DeviceRegistry<u32> = DeviceRegistry::new();
        registry.register("pc".to_string(), 1);
        registry.register("phone".to_string(), 2);

        let mut snapshot = registry.snapshot();
        snapshot.sort();

        assert_eq!(snapshot, vec![1, 2]);
    }

    #[test]
    fn the_registry_replaces_a_device_that_reconnects_with_the_same_id() {
        let registry: DeviceRegistry<u32> = DeviceRegistry::new();
        registry.register("pc".to_string(), 1);
        registry.register("pc".to_string(), 2);

        assert_eq!(registry.snapshot(), vec![2]);
    }

    #[test]
    fn a_removed_device_no_longer_appears_in_the_snapshot() {
        let registry: DeviceRegistry<u32> = DeviceRegistry::new();
        registry.register("pc".to_string(), 1);

        registry.remove("pc");

        assert!(registry.snapshot().is_empty());
    }
}
