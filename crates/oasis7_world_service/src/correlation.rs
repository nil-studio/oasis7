//! Runtime-owned correlation; node-frame encoding is a host adapter.
pub use oasis7_runtime::service_admission::correlation::*;

/// Register the intent inside the existing opaque RuntimeAction node frame.
#[cfg(not(target_arch = "wasm32"))]
pub fn encode_consensus_intent(
    request: &crate::SubmitIntentRequest<crate::WorldServicePayloadV1>,
) -> Result<Vec<u8>, String> {
    use crate::consensus_action_payload::*;
    encode_consensus_action_payload(&ConsensusActionPayloadEnvelope {
        version: 1,
        auth: None,
        gameplay_submission_origin: None,
        body: ConsensusActionPayloadBody::RuntimeAction {
            action: oasis7_runtime::Action::WorldServiceIntent {
                request: serde_json::to_value(request).map_err(|error| error.to_string())?,
            },
        },
    })
}

/// New explicit catalog body. The embedded Owner signature is not a Native key.
#[cfg(not(target_arch = "wasm32"))]
pub fn encode_cleanup_consensus_v2(
    request: &oasis7_client_api::world_service::task_cleanup_v2::TaskCleanupSubmitV2,
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
