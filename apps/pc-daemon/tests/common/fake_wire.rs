use std::collections::VecDeque;

use device_protocol::{DeviceMessage, NalaMessage};
use pc_daemon::client::{DaemonError, DeviceWire};

/// An in-memory `DeviceWire`: `recv()` pops scripted `NalaMessage`s (`None`
/// once exhausted, simulating the connection closing) and `send()` records
/// every `DeviceMessage` the session sent, in order.
pub struct FakeDeviceWire {
    pub incoming: VecDeque<NalaMessage>,
    pub sent: Vec<DeviceMessage>,
}

impl FakeDeviceWire {
    pub fn new(incoming: Vec<NalaMessage>) -> Self {
        Self {
            incoming: incoming.into(),
            sent: Vec::new(),
        }
    }
}

impl DeviceWire for FakeDeviceWire {
    fn send(&mut self, message: &DeviceMessage) -> Result<(), DaemonError> {
        self.sent.push(message.clone());
        Ok(())
    }

    fn recv(&mut self) -> Result<Option<NalaMessage>, DaemonError> {
        Ok(self.incoming.pop_front())
    }
}
