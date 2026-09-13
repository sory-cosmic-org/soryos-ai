//! Voice overlay helpers: status line and state transitions.

use assistant_core::VoiceState;

/// Status line for the microphone button / voice overlay.
pub fn status_line(state: VoiceState) -> String {
    format!("{} {}", state.icon(), state.label())
}

/// Allowed transitions of the voice state machine (guards UI glitches).
pub fn can_transition(from: VoiceState, to: VoiceState) -> bool {
    use VoiceState as V;
    matches!(
        (from, to),
        (V::Idle, V::Listening)
            | (V::Listening, V::Processing)
            | (V::Listening, V::Idle)
            | (V::Processing, V::Speaking)
            | (V::Processing, V::Idle)
            | (V::Processing, V::Error)
            | (V::Speaking, V::Idle)
            | (V::Speaking, V::Interrupted)
            | (V::Interrupted, V::Listening)
            | (V::Interrupted, V::Idle)
            | (V::Error, V::Idle)
            | (_, V::Error)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_to_listening_is_allowed() {
        assert!(can_transition(VoiceState::Idle, VoiceState::Listening));
        assert!(!can_transition(VoiceState::Idle, VoiceState::Speaking));
    }
}
