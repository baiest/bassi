use std::sync::{Arc, Mutex};

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
