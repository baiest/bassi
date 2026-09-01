use device_capabilities::registry::CapabilityRegistry;
use device_protocol::{
    DeviceMessage, DeviceState, NalaMessage, Outcome, PROTOCOL_VERSION, RejectReason,
};

use crate::client::{DaemonError, DeviceWire};
use crate::config::DeviceIdentity;
use crate::overlay_channel::OverlayChannel;

/// How a session ended: the connection closed cleanly, or Nala rejected
/// the `Hello` outright. Distinct from `DaemonError`, which is a transport
/// failure — a rejection is a valid protocol outcome, not an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionOutcome {
    Closed,
    Rejected(RejectReason),
}

/// Runs one connection's session: send `Hello` announcing every capability
/// `registry` currently allows, then loop answering `Invoke` (by running the
/// capability through `registry`) and `Ping` until the connection closes or
/// Nala rejects the handshake. A capability that fails is reported back as
/// an error `Result` — it never ends the session, since one bad call
/// shouldn't cut off every capability that follows it. Each `Invoke` also
/// pushes `Executing` then `Idle`/`Error` to `overlay`, so a local overlay
/// subscriber sees the daemon's state change in step with the turn.
pub fn run_session<W: DeviceWire>(
    wire: &mut W,
    registry: &mut CapabilityRegistry,
    identity: &DeviceIdentity,
    overlay: &OverlayChannel,
) -> Result<SessionOutcome, DaemonError> {
    wire.send(&DeviceMessage::Hello {
        protocol_version: PROTOCOL_VERSION,
        device_id: identity.device_id.clone(),
        name: identity.name.clone(),
        platform: identity.platform.clone(),
        token: identity.token.clone(),
        capabilities: registry.definitions(),
    })?;

    loop {
        match wire.recv()? {
            None => return Ok(SessionOutcome::Closed),
            Some(NalaMessage::Welcome { .. }) => {}
            Some(NalaMessage::Reject { reason }) => return Ok(SessionOutcome::Rejected(reason)),
            Some(NalaMessage::Invoke {
                request_id,
                capability,
                arguments,
            }) => {
                overlay.set_state(DeviceState::Executing);
                let outcome = registry.invoke(&capability, &arguments);
                let next_state = match outcome {
                    Outcome::Ok { .. } => DeviceState::Idle,
                    Outcome::Err { .. } => DeviceState::Error,
                };
                wire.send(&DeviceMessage::Result {
                    request_id,
                    outcome,
                })?;
                overlay.set_state(next_state);
            }
            Some(NalaMessage::Ping { id }) => {
                wire.send(&DeviceMessage::Pong { id })?;
            }
            // Nala's own turn state (listening/thinking/speaking), not
            // tied to whether this device's own capability is running —
            // forwarded as-is so the overlay reflects the whole turn.
            Some(NalaMessage::State { state }) => {
                overlay.set_state(state);
            }
        }
    }
}
