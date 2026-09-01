use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_protocol::{Event, EventSink, TaskId, TurnState};
use device_protocol::DeviceState;
use nala::adapters::devices::state_broadcast::DeviceStateBroadcaster;
use nala::application::devices::registry::DeviceRegistry;

use crate::fake_device::FakeDevice;

#[derive(Clone, Default)]
struct RecordingSink {
    events: Arc<Mutex<Vec<Event>>>,
}

impl RecordingSink {
    fn new() -> Self {
        Self::default()
    }

    fn events(&self) -> Vec<Event> {
        self.events.lock().unwrap().clone()
    }
}

impl EventSink for RecordingSink {
    fn emit(&mut self, event: Event) {
        self.events.lock().unwrap().push(event);
    }
}

fn state_changed(state: TurnState) -> Event {
    Event::StateChanged {
        task_id: TaskId::new(),
        state,
    }
}

#[test]
fn pushes_the_mapped_state_to_every_connected_device() {
    let registry: DeviceRegistry<FakeDevice> = DeviceRegistry::new();
    registry.register("pc".to_string(), FakeDevice::new("pc"));
    registry.register("phone".to_string(), FakeDevice::new("phone"));
    let registry = Arc::new(registry);

    let mut broadcaster = DeviceStateBroadcaster::new(RecordingSink::new(), Arc::clone(&registry));

    broadcaster.emit(state_changed(TurnState::Responding));

    for device in registry.snapshot() {
        assert_eq!(device.pushed_states(), vec![DeviceState::Speaking]);
    }
}

#[test]
fn every_event_is_still_forwarded_to_the_inner_sink_unchanged() {
    let registry: DeviceRegistry<FakeDevice> = DeviceRegistry::new();
    let registry = Arc::new(registry);
    let inner = RecordingSink::new();
    let mut broadcaster = DeviceStateBroadcaster::new(inner.clone(), registry);

    broadcaster.emit(state_changed(TurnState::Thinking));
    broadcaster.emit(Event::Cancelled {
        task_id: TaskId::new(),
    });

    assert_eq!(inner.events().len(), 2);
}

/// Polls `device.pushed_states()` until it has at least `count` entries, or
/// panics after a deadline — `Event::Greeting`'s `Idle` push happens on its
/// own thread after a short sleep, so it isn't visible immediately after
/// `emit` returns.
fn wait_for_pushed_states(device: &FakeDevice, count: usize) -> Vec<DeviceState> {
    for _ in 0..1000 {
        let pushed = device.pushed_states();
        if pushed.len() >= count {
            return pushed;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("expected at least {count} pushed state(s) within the deadline");
}

#[test]
fn a_greeting_pushes_speaking_immediately_to_every_connected_device() {
    let registry: DeviceRegistry<FakeDevice> = DeviceRegistry::new();
    registry.register("pc".to_string(), FakeDevice::new("pc"));
    let registry = Arc::new(registry);

    let mut broadcaster = DeviceStateBroadcaster::new(RecordingSink::new(), Arc::clone(&registry));

    broadcaster.emit(Event::Greeting {
        text: "hola".to_string(),
    });

    let device = registry.snapshot().into_iter().next().unwrap();
    assert_eq!(device.pushed_states().first(), Some(&DeviceState::Speaking));
}

#[test]
fn a_greeting_pushes_idle_after_the_estimated_speaking_time() {
    let registry: DeviceRegistry<FakeDevice> = DeviceRegistry::new();
    registry.register("pc".to_string(), FakeDevice::new("pc"));
    let registry = Arc::new(registry);

    let mut broadcaster = DeviceStateBroadcaster::new(RecordingSink::new(), Arc::clone(&registry));

    // Empty text keeps the estimated speaking time at zero, so the test
    // doesn't have to wait for a realistic greeting's duration.
    broadcaster.emit(Event::Greeting {
        text: String::new(),
    });

    let device = registry.snapshot().into_iter().next().unwrap();
    assert_eq!(
        wait_for_pushed_states(&device, 2),
        vec![DeviceState::Speaking, DeviceState::Idle]
    );
}

#[test]
fn a_greeting_with_no_devices_connected_does_not_panic() {
    let registry: DeviceRegistry<FakeDevice> = DeviceRegistry::new();
    let registry = Arc::new(registry);

    let mut broadcaster = DeviceStateBroadcaster::new(RecordingSink::new(), registry);

    broadcaster.emit(Event::Greeting {
        text: "hola".to_string(),
    });
}

#[test]
fn a_greeting_is_still_forwarded_to_the_inner_sink() {
    let registry: DeviceRegistry<FakeDevice> = DeviceRegistry::new();
    let registry = Arc::new(registry);
    let inner = RecordingSink::new();
    let mut broadcaster = DeviceStateBroadcaster::new(inner.clone(), registry);

    broadcaster.emit(Event::Greeting {
        text: "hola".to_string(),
    });

    assert_eq!(inner.events().len(), 1);
}

#[test]
fn non_state_changed_events_push_nothing_to_devices() {
    let registry: DeviceRegistry<FakeDevice> = DeviceRegistry::new();
    registry.register("pc".to_string(), FakeDevice::new("pc"));
    let registry = Arc::new(registry);

    let mut broadcaster = DeviceStateBroadcaster::new(RecordingSink::new(), Arc::clone(&registry));

    broadcaster.emit(Event::Cancelled {
        task_id: TaskId::new(),
    });

    let device = registry.snapshot().into_iter().next().unwrap();
    assert!(device.pushed_states().is_empty());
}
