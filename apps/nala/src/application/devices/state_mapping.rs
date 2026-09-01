use agent_protocol::TurnState;
use device_protocol::DeviceState;

/// Maps Nala's own turn state to the `DeviceState` pushed to connected
/// devices, so a local overlay reflects Nala's whole turn lifecycle
/// (listening, thinking, speaking) rather than only the moments a
/// device's own capability happens to be running.
pub fn turn_state_to_device_state(state: TurnState) -> DeviceState {
    match state {
        TurnState::Receiving => DeviceState::Listening,
        TurnState::Planning | TurnState::Thinking | TurnState::Verifying => DeviceState::Thinking,
        TurnState::Executing => DeviceState::Executing,
        TurnState::Responding => DeviceState::Speaking,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receiving_maps_to_listening() {
        assert_eq!(
            turn_state_to_device_state(TurnState::Receiving),
            DeviceState::Listening
        );
    }

    #[test]
    fn planning_thinking_and_verifying_all_map_to_thinking() {
        assert_eq!(
            turn_state_to_device_state(TurnState::Planning),
            DeviceState::Thinking
        );
        assert_eq!(
            turn_state_to_device_state(TurnState::Thinking),
            DeviceState::Thinking
        );
        assert_eq!(
            turn_state_to_device_state(TurnState::Verifying),
            DeviceState::Thinking
        );
    }

    #[test]
    fn executing_maps_to_executing() {
        assert_eq!(
            turn_state_to_device_state(TurnState::Executing),
            DeviceState::Executing
        );
    }

    #[test]
    fn responding_maps_to_speaking() {
        assert_eq!(
            turn_state_to_device_state(TurnState::Responding),
            DeviceState::Speaking
        );
    }
}
