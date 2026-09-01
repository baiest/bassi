use std::sync::Arc;
use std::thread;
use std::time::Duration;

use agent_protocol::{Event, EventSink};
use device_protocol::DeviceState;

use crate::application::devices::registry::DeviceRegistry;
use crate::application::devices::state_mapping::turn_state_to_device_state;
use crate::ports::device::RemoteDevice;

/// How many characters per second the greeting is assumed to take to speak.
/// A device never sees this text or hears this audio (`voice` is the one
/// actually speaking it) — this is only used to keep a connected device's
/// overlay showing `Speaking` for roughly as long as `voice` would be.
const GREETING_CHARS_PER_SECOND: f64 = 15.0;

fn estimate_speaking_duration(text: &str) -> Duration {
    Duration::from_secs_f64(text.chars().count() as f64 / GREETING_CHARS_PER_SECOND)
}

/// Forwards every event to `inner` unchanged, and additionally pushes a
/// mapped `DeviceState` to every currently-connected device whenever
/// Nala's own turn state changes — so a local overlay reflects Nala's
/// whole turn lifecycle (listening, thinking, speaking), not just the
/// moments a device's own capability happens to be running. Also mirrors
/// `Event::Greeting` the same way (`Speaking` now, `Idle` once the
/// estimated speaking time has passed): it isn't part of any turn, so it
/// never produces a `StateChanged`, but a device's overlay should still
/// react to it.
pub struct DeviceStateBroadcaster<E, D: RemoteDevice + Clone> {
    inner: E,
    devices: Arc<DeviceRegistry<D>>,
}

impl<E, D: RemoteDevice + Clone> DeviceStateBroadcaster<E, D> {
    pub fn new(inner: E, devices: Arc<DeviceRegistry<D>>) -> Self {
        Self { inner, devices }
    }
}

impl<E: EventSink, D: RemoteDevice + Clone + Send + 'static> EventSink
    for DeviceStateBroadcaster<E, D>
{
    fn emit(&mut self, event: Event) {
        match &event {
            Event::StateChanged { state, .. } => {
                let mapped = turn_state_to_device_state(*state);
                for device in self.devices.snapshot() {
                    device.push_state(mapped);
                }
            }
            Event::Greeting { text } => {
                let snapshot = self.devices.snapshot();
                if !snapshot.is_empty() {
                    for device in &snapshot {
                        device.push_state(DeviceState::Speaking);
                    }
                    let duration = estimate_speaking_duration(text);
                    // Runs on its own thread so a slow-to-speak greeting
                    // never delays the rest of this connection's session.
                    thread::spawn(move || {
                        thread::sleep(duration);
                        for device in &snapshot {
                            device.push_state(DeviceState::Idle);
                        }
                    });
                }
            }
            _ => {}
        }
        self.inner.emit(event);
    }
}
