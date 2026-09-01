use std::sync::Arc;

use agent_protocol::{Event, EventSink};

use crate::application::devices::registry::DeviceRegistry;
use crate::application::devices::state_mapping::turn_state_to_device_state;
use crate::ports::device::RemoteDevice;

/// Forwards every event to `inner` unchanged, and additionally pushes a
/// mapped `DeviceState` to every currently-connected device whenever
/// Nala's own turn state changes — so a local overlay reflects Nala's
/// whole turn lifecycle (listening, thinking, speaking), not just the
/// moments a device's own capability happens to be running.
pub struct DeviceStateBroadcaster<E, D: RemoteDevice + Clone> {
    inner: E,
    devices: Arc<DeviceRegistry<D>>,
}

impl<E, D: RemoteDevice + Clone> DeviceStateBroadcaster<E, D> {
    pub fn new(inner: E, devices: Arc<DeviceRegistry<D>>) -> Self {
        Self { inner, devices }
    }
}

impl<E: EventSink, D: RemoteDevice + Clone> EventSink for DeviceStateBroadcaster<E, D> {
    fn emit(&mut self, event: Event) {
        if let Event::StateChanged { state, .. } = &event {
            let mapped = turn_state_to_device_state(*state);
            for device in self.devices.snapshot() {
                device.push_state(mapped);
            }
        }
        self.inner.emit(event);
    }
}
