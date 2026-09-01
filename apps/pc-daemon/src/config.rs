/// This daemon's identity, sent in its `Hello` on every connection attempt.
/// `device_id` should stay stable across restarts so Nala's registry can
/// tell a reconnect apart from a second device.
pub struct DeviceIdentity {
    pub device_id: String,
    pub name: String,
    pub platform: String,
    pub token: String,
}
