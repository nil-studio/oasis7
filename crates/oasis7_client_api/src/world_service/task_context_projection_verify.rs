//! Public proof validation never receives a private Context slot.
use super::*;
use oasis7_agent_api::game::{GameTaskFenceV1, TaskContextAuthorityModeV1};

fn native(context: &TaskContextIdentityV1) -> Result<(), String> {
    context.validate()?;
    if context.authority_mode != Some(TaskContextAuthorityModeV1::NativeExecutorV1) {
        return Err("projection profile is not Native".into());
    }
    Ok(())
}
fn commit(commit: &CommitRef, fence: &GameTaskFenceV1) -> Result<(), String> {
    commit.validate().map_err(|e| e.to_string())?;
    if commit.world.world_id != fence.world_id
        || commit.world.genesis_digest != fence.genesis_digest
        || commit.binding.branch_id != fence.branch_id
        || commit.binding.reorg_generation != fence.reorg_epoch.0
    {
        return Err("projection World history mismatch".into());
    }
    Ok(())
}
fn proof_material(proof: &ContextPrivateResponseProofV2) -> Result<(), String> {
    if !digest(&proof.request_digest)
        || proof.signature_hex.len() != 128
        || !proof
            .signature_hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("invalid private response signature material".into());
    }
    Ok(())
}
fn digest(value: &str) -> bool {
    value.strip_prefix("blake3:").is_some_and(nonce)
}
fn base(binding: &RuntimeCognitionBaseBindingV1, fence: &GameTaskFenceV1) -> Result<(), String> {
    binding.validate().map_err(|e| e.to_string())?;
    if binding.world_id != fence.world_id
        || binding.branch_id != fence.branch_id
        || binding.reorg_epoch != fence.reorg_epoch.0
    {
        return Err("projection base binding mismatch".into());
    }
    Ok(())
}
pub fn validate_prepare_attestation_request_v2(
    q: &TaskContextPrepareAttestationRequestV2,
) -> Result<(), String> {
    verify_prepare(&q.original)?;
    if q.original.request.authority_mode != Some(TaskContextAuthorityModeV1::NativeExecutorV1)
        || !nonce(&q.observation_nonce)
        || q.observation_nonce == q.original.request.client_nonce
    {
        return Err("invalid Native prepared projection request".into());
    }
    Ok(())
}
pub fn verify_prepared_projection_v2<'a>(
    q: &TaskContextPrepareAttestationRequestV2,
    response: &'a WorldContextAttestationV2<TaskContextPreparedProjectionV2>,
    world_key: &str,
) -> Result<VerifiedPreparedProjectionV2<'a>, String> {
    validate_prepare_attestation_request_v2(q)?;
    verify_attestation(PREPARED_PROJECTION_DOMAIN_V2, q, response, world_key)?;
    let p = &response.payload;
    native(&p.context)?;
    p.body.validate()?;
    proof_material(&p.private_response)?;
    commit(&p.read_commit, &q.original.request.fence)?;
    commit(&p.activation_commit, &q.original.request.fence)?;
    base(&p.captured_base_binding, &q.original.request.fence)?;
    p.context
        .validate_capture_deadline(p.captured_base_binding.base_tick)?;
    if p.api_version != 2
        || p.original != q.original
        || p.observation_nonce != q.observation_nonce
        || p.context.fence != q.original.request.fence
        || p.private_response.request_digest
            != request_digest(TASK_CONTEXT_PREPARE_PATH, &q.original)?
        || p.activation_commit.position > p.read_commit.position
        || (p.activation_commit.position == p.read_commit.position
            && p.activation_commit != p.read_commit)
    {
        return Err("prepared projection exact tuple mismatch".into());
    }
    Ok(VerifiedPreparedProjectionV2 { projection: p })
}
pub fn validate_get_attestation_request_v2(
    q: &TaskContextGetAttestationRequestV2,
    world_key: &str,
) -> Result<(), String> {
    let prepared = &q.prepared_projection.payload;
    let previous_request = TaskContextPrepareAttestationRequestV2 {
        original: prepared.original.clone(),
        observation_nonce: prepared.observation_nonce.clone(),
    };
    verify_prepared_projection_v2(&previous_request, &q.prepared_projection, world_key)?;
    verify_get(&q.original)?;
    if !nonce(&q.observation_nonce)
        || q.observation_nonce == prepared.observation_nonce
        || q.observation_nonce == prepared.original.request.client_nonce
        || q.original.request.context != prepared.context
        || q.original.request.prepare_operation_id != prepared.original.request.client_operation_id
        || q.original.subject_public_key != prepared.original.subject_public_key
    {
        return Err("captured projection request mismatch".into());
    }
    Ok(())
}
pub fn verify_captured_projection_v2(
    q: &TaskContextGetAttestationRequestV2,
    response: &WorldContextAttestationV2<TaskContextCapturedProjectionV2>,
    world_key: &str,
) -> Result<(), String> {
    validate_get_attestation_request_v2(q, world_key)?;
    verify_attestation(CAPTURED_PROJECTION_DOMAIN_V2, q, response, world_key)?;
    let p = &response.payload;
    native(&p.context)?;
    p.body.validate()?;
    proof_material(&p.private_response)?;
    commit(&p.capture_commit, &p.context.fence)?;
    commit(&p.read_commit, &p.context.fence)?;
    p.prefix_correlation.validate().map_err(|e| e.to_string())?;
    if p.api_version != 2
        || p.original != q.original
        || p.observation_nonce != q.observation_nonce
        || p.prepared_projection_digest != public_projection_digest_v2(&q.prepared_projection)?
        || p.context != q.original.request.context
        || p.private_response.request_digest != request_digest(TASK_CONTEXT_READ_PATH, &q.original)?
        || !digest(&p.prefix_original_digest)
        || p.prefix_correlation.key.world != p.read_commit.world
        || p.capture_commit.position > p.read_commit.position
        || (p.capture_commit.position == p.read_commit.position
            && p.capture_commit != p.read_commit)
        || p.read_logical_tick.0 > p.context.valid_until_tick.0
    {
        return Err("captured projection exact tuple mismatch".into());
    }
    Ok(())
}
pub fn validate_move_projection_request_v2(
    q: &TaskContextMoveProjectionRequestV2,
    world_key: &str,
) -> Result<(), String> {
    let captured = &q.captured_projection.payload;
    let previous_request = TaskContextGetAttestationRequestV2 {
        original: q.original.clone(),
        observation_nonce: captured.observation_nonce.clone(),
        prepared_projection: q.prepared_projection.clone(),
    };
    verify_captured_projection_v2(&previous_request, &q.captured_projection, world_key)?;
    if !nonce(&q.observation_nonce)
        || q.observation_nonce == captured.observation_nonce
        || q.observation_nonce == q.prepared_projection.payload.observation_nonce
        || q.observation_nonce == q.prepared_projection.payload.original.request.client_nonce
    {
        return Err("invalid independent Move projection nonce".into());
    }
    Ok(())
}
pub fn verify_move_projection_v2<'a>(
    q: &TaskContextMoveProjectionRequestV2,
    response: &'a WorldContextAttestationV2<TaskContextMoveProjectionV2>,
    world_key: &str,
) -> Result<VerifiedMoveProjectionV2<'a>, String> {
    validate_move_projection_request_v2(q, world_key)?;
    verify_attestation(CONTEXT_MOVE_PROJECTION_PATH_V2, q, response, world_key)?;
    let p = &response.payload;
    native(&p.context)?;
    p.body.validate()?;
    commit(&p.capture_commit, &p.context.fence)?;
    commit(&p.read_commit, &p.context.fence)?;
    base(&p.captured_base_binding, &p.context.fence)?;
    if p.api_version != 2
        || p.observation_nonce != q.observation_nonce
        || p.original_get_digest != request_digest(TASK_CONTEXT_READ_PATH, &q.original)?
        || p.prepared_projection_digest != public_projection_digest_v2(&q.prepared_projection)?
        || p.captured_projection_digest != public_projection_digest_v2(&q.captured_projection)?
        || p.context != q.original.request.context
        || p.capture_commit != q.captured_projection.payload.capture_commit
        || p.read_logical_tick.0 != p.captured_base_binding.base_tick
        || p.read_logical_tick.0 > p.context.valid_until_tick.0
        || p.capture_commit.position > p.read_commit.position
        || (p.capture_commit.position == p.read_commit.position
            && p.capture_commit != p.read_commit)
        || !digest(&p.observation_digest)
        || !digest(&p.capability_snapshot_hash)
        || !digest(&p.authority_context_hash)
    {
        return Err("Move projection exact tuple mismatch".into());
    }
    Ok(VerifiedMoveProjectionV2 { projection: p })
}

/// Identity must verify both actual signatures and the complete old private
/// body commitment before sealing. Account must never receive this input.
pub fn verify_prepared_delivery_v2(
    q: &TaskContextPrepareAttestationRequestV2,
    response: &WorldContextAttestationV2<TaskContextPreparedDeliveryV2>,
    world_key: &str,
) -> Result<(), String> {
    validate_prepare_attestation_request_v2(q)?;
    verify_attestation(CONTEXT_PREPARE_ATTESTED_PATH_V2, q, response, world_key)?;
    verify_prepared_projection_v2(q, &response.payload.projection, world_key)?;
    let exact = derive_prepared_projection_v2(q, &response.payload.private_body, world_key)?;
    if exact != response.payload.projection.payload {
        return Err("prepared delivery body/projection mismatch".into());
    }
    delivery_wire(response)
}
pub fn verify_captured_delivery_v2(
    q: &TaskContextGetAttestationRequestV2,
    response: &WorldContextAttestationV2<TaskContextCapturedDeliveryV2>,
    world_key: &str,
) -> Result<(), String> {
    validate_get_attestation_request_v2(q, world_key)?;
    verify_attestation(CONTEXT_READ_ATTESTED_PATH_V2, q, response, world_key)?;
    verify_captured_projection_v2(q, &response.payload.projection, world_key)?;
    let exact = derive_captured_projection_v2(q, &response.payload.private_body, world_key)?;
    if exact != response.payload.projection.payload {
        return Err("captured delivery body/projection mismatch".into());
    }
    delivery_wire(response)
}
fn delivery_wire<T: Serialize>(response: &WorldContextAttestationV2<T>) -> Result<(), String> {
    let wire = zeroize::Zeroizing::new(serde_json::to_vec(response).map_err(|e| e.to_string())?);
    if wire.len() > MAX_ATTESTED_CONTEXT_WIRE_BYTES_V2 {
        return Err("context_too_large".into());
    }
    Ok(())
}
