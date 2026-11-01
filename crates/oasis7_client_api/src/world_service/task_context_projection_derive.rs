//! Producer derivations operate on actual complete private bodies at World.
use super::*;
use oasis7_agent_api::game::TaskContextAuthorityModeV1;
use oasis7_agent_api::world_action::{Action, GeoPos};
use oasis7_agent_api::world_cognition::{
    RuntimeCognitionCommitRequestV1, RuntimeCognitionResponseArtifactV1,
};

pub fn derive_prepared_projection_v2(
    request: &TaskContextPrepareAttestationRequestV2,
    body: &SignedServiceResponse<PreparedTaskContextV1>,
    world_key: &str,
) -> Result<TaskContextPreparedProjectionV2, String> {
    validate_prepare_attestation_request_v2(request)?;
    verify_private_reply(
        TASK_CONTEXT_PREPARE_PATH,
        &request.original,
        body,
        world_key,
    )?;
    body.payload.validate_against(&request.original)?;
    Ok(TaskContextPreparedProjectionV2 {
        api_version: 2,
        profile: ContextProjectionProfileV2::NativeWorldContextV2,
        original: request.original.clone(),
        observation_nonce: request.observation_nonce.clone(),
        context: body.payload.identity.clone(),
        read_commit: body.payload.snapshot.read_commit.clone(),
        activation_commit: body.payload.snapshot.activation_commit.clone(),
        captured_base_binding: body.payload.snapshot.captured_base_binding.clone(),
        private_response: private_proof(body),
        body: ContextBodyCommitmentV2::from_signed_body(body)?,
    })
}
fn private_proof<T>(body: &SignedServiceResponse<T>) -> ContextPrivateResponseProofV2 {
    ContextPrivateResponseProofV2 {
        request_digest: body.request_digest.clone(),
        signature_hex: body.signature_hex.clone(),
    }
}
fn exact_prepared(
    projection: &TaskContextPreparedProjectionV2,
    prepared: &PreparedTaskContextV1,
    world_key: &str,
) -> Result<(), String> {
    let original = SignedServiceResponse {
        request_digest: projection.private_response.request_digest.clone(),
        payload: prepared.clone(),
        signature_hex: projection.private_response.signature_hex.clone(),
    };
    verify_private_reply(
        TASK_CONTEXT_PREPARE_PATH,
        &projection.original,
        &original,
        world_key,
    )?;
    prepared.validate_against(&projection.original)?;
    if ContextBodyCommitmentV2::from_signed_body(&original)? != projection.body {
        return Err("prepared projection commitment mismatch".into());
    }
    Ok(())
}
pub fn derive_captured_projection_v2(
    request: &TaskContextGetAttestationRequestV2,
    body: &SignedServiceResponse<CapturedTaskContextV1>,
    world_key: &str,
) -> Result<TaskContextCapturedProjectionV2, String> {
    validate_get_attestation_request_v2(request, world_key)?;
    verify_private_reply(TASK_CONTEXT_READ_PATH, &request.original, body, world_key)?;
    body.payload.validate_against(&request.original)?;
    exact_prepared(
        &request.prepared_projection.payload,
        &body.payload.prepared,
        world_key,
    )?;
    Ok(TaskContextCapturedProjectionV2 {
        api_version: 2,
        profile: ContextProjectionProfileV2::NativeWorldContextV2,
        original: request.original.clone(),
        observation_nonce: request.observation_nonce.clone(),
        prepared_projection_digest: public_projection_digest_v2(&request.prepared_projection)?,
        context: body.payload.prepared.identity.clone(),
        capture_commit: body.payload.capture_commit.clone(),
        read_commit: body.payload.current_public_context.read_commit.clone(),
        read_logical_tick: oasis7_agent_api::game::DecimalU64(
            body.payload
                .current_public_context
                .captured_base_binding
                .base_tick,
        ),
        prefix_original_digest: request_digest(
            "custody-context-prefix-original/v2",
            &body.payload.capture_proof.request,
        )?,
        prefix_correlation: body.payload.capture_proof.request.correlation.clone(),
        private_response: private_proof(body),
        body: ContextBodyCommitmentV2::from_signed_body(body)?,
    })
}
pub fn derive_move_projection_v2(
    request: &TaskContextMoveProjectionRequestV2,
    body: &SignedServiceResponse<CapturedTaskContextV1>,
    world_key: &str,
) -> Result<TaskContextMoveProjectionV2, String> {
    validate_move_projection_request_v2(request, world_key)?;
    let old_live_request = TaskContextLiveReadV1 {
        original: request.original.clone(),
        observation_nonce: request.observation_nonce.clone(),
    };
    verify_live_context_reply(&old_live_request, body, world_key)?;
    exact_prepared(
        &request.prepared_projection.payload,
        &body.payload.prepared,
        world_key,
    )?;
    let previous = &request.captured_projection.payload;
    if body.payload.capture_commit != previous.capture_commit
        || request_digest(
            "custody-context-prefix-original/v2",
            &body.payload.capture_proof.request,
        )? != previous.prefix_original_digest
        || body.payload.capture_proof.request.correlation != previous.prefix_correlation
    {
        return Err("live projection capture lineage mismatch".into());
    }
    let public = &body.payload.current_public_context;
    let authority = public.authority()?;
    let (_, capability_snapshot_hash, authority_context_hash) = authority.hashes();
    Ok(TaskContextMoveProjectionV2 {
        api_version: 2,
        profile: ContextProjectionProfileV2::NativeWorldContextV2,
        observation_nonce: request.observation_nonce.clone(),
        original_get_digest: request_digest(TASK_CONTEXT_READ_PATH, &request.original)?,
        prepared_projection_digest: public_projection_digest_v2(&request.prepared_projection)?,
        captured_projection_digest: public_projection_digest_v2(&request.captured_projection)?,
        context: body.payload.prepared.identity.clone(),
        capture_commit: body.payload.capture_commit.clone(),
        read_commit: public.read_commit.clone(),
        read_logical_tick: oasis7_agent_api::game::DecimalU64(
            public.captured_base_binding.base_tick,
        ),
        captured_base_binding: public.captured_base_binding.clone(),
        observation_digest: request_digest(
            "game-task-public-observation/v1",
            &public.actor_observation,
        )?,
        capability_snapshot_hash: capability_snapshot_hash.into(),
        authority_context_hash: authority_context_hash.into(),
        body: ContextBodyCommitmentV2::from_signed_body(body)?,
    })
}
pub(super) fn move_candidate(
    p: &TaskContextMoveProjectionV2,
    position_cm: [i64; 3],
) -> Result<CognitionIntentV1, String> {
    let context = &p.context;
    if context.authority_mode != Some(TaskContextAuthorityModeV1::NativeExecutorV1) {
        return Err("projection is not Native".into());
    }
    let action = Action::MoveAgent {
        agent_id: context.fence.agent_id.clone(),
        to: GeoPos::new(position_cm[0], position_cm[1], position_cm[2]),
    };
    let request = RuntimeCognitionCommitRequestV1 {
        agent_id: context.fence.agent_id.clone(),
        agent_session_id: context.agent_session_id.clone(),
        agent_turn_id: context.agent_turn_id.clone(),
        decision_request_id: context.decision_request_id.clone(),
        retry_seq: context.retry_seq,
        transport_attempt: context.transport_attempt,
        request_digest: context.request_commitment.clone(),
        observation_digest: p.observation_digest.clone(),
        context_digest: context.context_commitment.clone(),
        capability_snapshot_hash: p.capability_snapshot_hash.clone(),
        authority_context_hash: p.authority_context_hash.clone(),
        captured_base_binding: p.captured_base_binding.clone(),
    };
    let mut response_artifact = RuntimeCognitionResponseArtifactV1 {
        schema_version: 1,
        context_discriminator: RuntimeCognitionResponseArtifactV1::CONTEXT_DISCRIMINATOR.into(),
        context_version: 1,
        agent_session_id: context.agent_session_id.clone(),
        agent_turn_id: context.agent_turn_id.clone(),
        decision_request_id: context.decision_request_id.clone(),
        retry_seq: context.retry_seq,
        transport_attempt: context.transport_attempt,
        request_digest: context.request_commitment.clone(),
        response_digest: request_digest(
            "game-task-public-action/v1",
            &(&context.request_commitment, &action),
        )?,
        artifact_digest: String::new(),
    };
    response_artifact.refresh_artifact_digest();
    request.validate().map_err(|e| e.to_string())?;
    response_artifact
        .validate_for_request(&request)
        .map_err(|e| e.to_string())?;
    Ok(CognitionIntentV1 {
        request,
        action,
        response_artifact,
        delegation_generation: context
            .fence
            .executor_binding
            .as_ref()
            .ok_or("missing Executor")?
            .delegation_generation
            .0,
        causal_proposal: None,
        task_fence: Some(context.fence.clone()),
    })
}
