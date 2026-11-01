//! Compatibility exports for Runtime-owned correlation.
use super::*;
pub use crate::runtime::service_admission::correlation::*;
/// Register the intent inside the existing opaque RuntimeAction node frame.
pub fn encode_consensus_intent(
    request: &super::SubmitIntentRequest<WorldServicePayloadV1>,
) -> Result<Vec<u8>, String> {
    use crate::consensus_action_payload::*;
    encode_consensus_action_payload(&ConsensusActionPayloadEnvelope {
        version: 1,
        auth: None,
        gameplay_submission_origin: None,
        body: ConsensusActionPayloadBody::RuntimeAction {
            action: crate::runtime::Action::WorldServiceIntent {
                request: serde_json::to_value(request).map_err(|error| error.to_string())?,
            },
        },
    })
}

/// New explicit catalog body. The embedded Owner signature is not a Native key.
pub fn encode_cleanup_consensus_v2(
    request: &task_cleanup_v2::TaskCleanupSubmitV2,
) -> Result<Vec<u8>, String> {
    use crate::consensus_action_payload::*;
    request.verify_owner_signature()?;
    encode_consensus_action_payload(&ConsensusActionPayloadEnvelope {
        version: 1,
        auth: None,
        gameplay_submission_origin: None,
        body: ConsensusActionPayloadBody::WorldCleanupV2 {
            request: request.clone(),
        },
    })
}

/// Separate guarded Stop body; this never wraps a Close or V1 intent.
pub fn encode_guarded_stop_consensus_v2(
    request: &task_stop_v2::TaskStopSubmitV2,
) -> Result<Vec<u8>, String> {
    use crate::consensus_action_payload::*;
    request.verify_owner_signature()?;
    encode_consensus_action_payload(&ConsensusActionPayloadEnvelope {
        version: 1,
        auth: None,
        gameplay_submission_origin: None,
        body: ConsensusActionPayloadBody::WorldGuardedStopV2 {
            request: Box::new(request.clone()),
        },
    })
}
