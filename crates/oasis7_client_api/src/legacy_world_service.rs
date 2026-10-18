//! Complete concrete legacy contract. Values confer no Runtime write authority.
pub type SchedulerOperationV1 = crate::world_service_wire::SchedulerOperationV1<
    oasis7_agent_api::ContinuousAgentRequestContextV1,
    oasis7_agent_api::legacy_economy::CognitionLeaseRequestV1,
    oasis7_agent_api::legacy_scheduler::CognitionContinuationProposalV1,
    oasis7_agent_api::legacy_scheduler::ContinuationStatusV1,
    oasis7_agent_api::legacy_scheduler::CognitionContinuationResumeRequestV1,
    oasis7_agent_api::legacy_scheduler::CognitionWakeDispositionV1,
    oasis7_agent_api::legacy_scheduler::CognitionContextDigestsV1,
>;
pub type SchedulerIntentV1 = crate::world_service_wire::SchedulerIntentV1<SchedulerOperationV1>;
pub type WorldServicePayloadV1 = crate::world_service_wire::WorldServicePayloadV1<
    oasis7_proto::viewer::AgentChatRequest,
    SchedulerIntentV1,
>;
pub type AuthenticatedLookup = crate::world_service_wire::AuthenticatedLookup<
    oasis7_proto::viewer::AgentChatRequest,
    SchedulerIntentV1,
>;
pub type CanonicalIntentResultV1 = crate::world_service_wire::CanonicalIntentResultV1<
    oasis7_proto::viewer::AgentChatRequest,
    SchedulerIntentV1,
>;

use crate::world_service::*;
/// Derives identity from verified signed material; server additionally checks
/// canonical ownership/delegation and expected world before accepting it.
pub fn derive_correlation(
    world: WorldIdentity,
    payload: &WorldServicePayloadV1,
) -> Result<RequestCorrelation, String> {
    crate::world_service_correlation::derive_correlation_with_legacy(world, payload, |payload| {
        Ok(match payload {
            WorldServicePayloadV1::GameplayJson(bytes) => {
                if let Ok(request) =
                    serde_json::from_slice::<oasis7_proto::viewer::GameplayActionRequest>(bytes)
                {
                    let proof = request.auth.as_ref().ok_or("missing gameplay proof")?;
                    let verified = oasis7_proto::player_auth::verify_gameplay_action_auth_proof(
                        &request, proof,
                    )?;
                    (
                        verified.public_key,
                        "gameplay",
                        "player_auth",
                        verified.nonce.to_string(),
                    )
                } else {
                    let command: oasis7_proto::viewer::CollectDataCommand =
                        serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
                    let oasis7_proto::viewer::CollectDataCommand::Submit { request } = &command
                    else {
                        return Err("unsupported collect_data operation".into());
                    };
                    let proof = request.auth.as_ref().ok_or("missing collect_data proof")?;
                    let verified =
                        oasis7_proto::player_auth::verify_collect_data_auth_proof(&command, proof)?;
                    (
                        verified.public_key,
                        "collect_data",
                        "collect_data_submit",
                        verified.nonce.to_string(),
                    )
                }
            }
            WorldServicePayloadV1::AgentChat(request) => {
                let proof = request
                    .auth
                    .as_ref()
                    .ok_or("missing canonical chat proof")?;
                let verified =
                    oasis7_proto::player_auth::verify_agent_chat_auth_proof(request, proof)?;
                (
                    verified.public_key,
                    "agent_chat",
                    "canonical_owner_chat",
                    verified.nonce.to_string(),
                )
            }
            _ => return Err("legacy verifier received non-browser payload".into()),
        })
    })
}

/// Validate the signed request identity and its pure receipt association.
pub fn validate_canonical_result(key: &str, value: &serde_json::Value) -> Result<(), String> {
    let result: CanonicalIntentResultV1 =
        serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    result.request.validate().map_err(|e| e.to_string())?;
    if derive_correlation(
        result.request.correlation.key.world.clone(),
        &result.request.signed_payload,
    )? != result.request.correlation
        || request_digest("request-key", &result.request.correlation.key)? != key
        || result.committed_height == 0
    {
        return Err("invalid canonical intent result identity".into());
    }
    crate::legacy_task_results::validate_result(&result)
}
