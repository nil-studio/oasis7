//! Persistence JSON to observation values; no hydration or authority validation.
use crate::WorldServiceContinuationContext;
use oasis7_world_state::AgentContinuation;
use serde_json::Value;

/// Identifies the failed read without changing its established public message.
#[derive(Debug)]
pub enum CognitionProjectionDecodeError {
    Continuations(serde_json::Error),
    ContinuationContext(serde_json::Error),
}

impl std::fmt::Display for CognitionProjectionDecodeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Continuations(error) | Self::ContinuationContext(error) => {
                std::fmt::Display::fmt(error, formatter)
            }
        }
    }
}

impl std::error::Error for CognitionProjectionDecodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Continuations(error) | Self::ContinuationContext(error) => Some(error),
        }
    }
}

/// Decode the complete raw accessor value before the Service filters by Agent.
/// Terminal and sparse legacy values keep their original fields/defaults.
pub fn decode_cognition_continuations(
    value: Value,
) -> Result<Vec<AgentContinuation>, CognitionProjectionDecodeError> {
    serde_json::from_value(value).map_err(CognitionProjectionDecodeError::Continuations)
}

/// Decode only an existing, referenced context; registry lookup stays Runtime's.
/// In particular, an explicit null item is an error, not a missing context.
pub fn decode_continuation_context(
    value: Value,
) -> Result<WorldServiceContinuationContext, CognitionProjectionDecodeError> {
    serde_json::from_value(value).map_err(CognitionProjectionDecodeError::ContinuationContext)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn failures_identify_read_position_without_changing_serde_message_or_source() {
        let continuation = decode_cognition_continuations(Value::Null).unwrap_err();
        let context = decode_continuation_context(Value::Null).unwrap_err();
        for error in [&continuation, &context] {
            assert_eq!(error.to_string(), error.source().unwrap().to_string());
        }
        assert!(matches!(
            continuation,
            CognitionProjectionDecodeError::Continuations(_)
        ));
        assert!(matches!(
            context,
            CognitionProjectionDecodeError::ContinuationContext(_)
        ));
    }
}
