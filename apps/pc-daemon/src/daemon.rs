use device_capabilities::registry::CapabilityRegistry;
use device_protocol::{DeviceMessage, NalaMessage, PROTOCOL_VERSION, RejectReason};

use crate::client::{DaemonError, DeviceWire};
use crate::config::DeviceIdentity;

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
/// shouldn't cut off every capability that follows it. The daemon has no
/// overlay of its own — that lives in `nala-overlay` now, connected
/// directly to `voice --serve` — so `NalaMessage::State` is simply
/// acknowledged and ignored here.
pub fn run_session<W: DeviceWire>(
    wire: &mut W,
    registry: &mut CapabilityRegistry,
    identity: &DeviceIdentity,
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
                let outcome = registry.invoke(&capability, &arguments);
                wire.send(&DeviceMessage::Result {
                    request_id,
                    outcome,
                })?;
            }
            Some(NalaMessage::Ping { id }) => {
                wire.send(&DeviceMessage::Pong { id })?;
            }
            // No overlay of its own to forward this to — see the doc
            // comment above.
            Some(NalaMessage::State { .. }) => {}
        }
    }
}
