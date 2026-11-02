//! Fixed V2 full-signed storage manifests. No arbitrary job constructor is exported.
use super::*;
use crate::context_body::ContextBodyError;
use crate::context_body_v2::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use oasis7_agent_api::game::GAME_GOAL_PREPARE_DOMAIN;
use oasis7_client_api::{
    world_service::{self as ws, task_context::*, task_context_projection::*},
    world_service_wire::WorldServicePayloadV1,
};
fn invalid() -> ApprovalAdmissionError {
    ApprovalAdmissionError::Invalid
}
fn job<T: Serialize>(
    domain: &str,
    value: &T,
    key: &str,
    signature: &str,
) -> Result<SignatureCheck> {
    let prepared = ws::private_goal::prepare_owner_signature_strict(domain, value, key, signature)
        .map_err(|_| invalid())?;
    prepared
        .verify_strict()
        .map_err(|_| ApprovalAdmissionError::SignatureRejected)?;
    Ok(SignatureCheck {
        key_id: format!("context-fullsigned-v2:{domain}"),
        public_key_hex: key.into(),
        signature_base64url: URL_SAFE_NO_PAD.encode(prepared.signature_bytes()),
        canonical_bytes: prepared.canonical_bytes().to_vec(),
    })
}
fn response<T: Serialize>(
    domain: &str,
    value: &ws::SignedServiceResponse<T>,
    key: &str,
) -> Result<SignatureCheck> {
    job(
        domain,
        &(&value.request_digest, &value.payload),
        key,
        &value.signature_hex,
    )
}
fn attestation<T: Serialize>(
    domain: &str,
    value: &WorldContextAttestationV2<T>,
    key: &str,
) -> Result<SignatureCheck> {
    job(
        domain,
        &(&value.request_digest, &value.payload),
        key,
        &value.signature_hex,
    )
}
fn prepared_jobs(
    q: &TaskContextPrepareAttestationRequestV2,
    body: &ws::SignedServiceResponse<PreparedTaskContextV1>,
    key: &str,
) -> Result<Vec<SignatureCheck>> {
    let owner = &body.payload.snapshot.active_goal.owner_proof;
    Ok(vec![
        job(
            TASK_CONTEXT_PREPARE_PATH,
            &q.original.request,
            &q.original.subject_public_key,
            &q.original.signature_hex,
        )?,
        job(
            GAME_GOAL_PREPARE_DOMAIN,
            &owner.request,
            &owner.subject_public_key,
            &owner.signature_hex,
        )?,
        response(TASK_CONTEXT_PREPARE_PATH, body, key)?,
    ])
}
fn bind<T: Serialize>(
    scope: &ContextBodyScopeV2,
    material: &T,
    checks: Vec<SignatureCheck>,
) -> Result<ExactSignatureChecks> {
    let binding = canonical_digest(
        "oasis7.custody.fullsigned-context.complete-checks.v2",
        &(scope, material, &checks),
    )?;
    Ok(ExactSignatureChecks {
        operation_id: scope.context_operation_id.clone(),
        body_digest: binding.clone(),
        context_digest: binding.clone(),
        verify_nonce: binding,
        verification_digest: None,
        checks,
    })
}
pub fn fullsigned_prepared_storage_checks_v2(
    scope: &ContextBodyScopeV2,
    world: &ApprovedWorldV1,
    q: &TaskContextPrepareAttestationRequestV2,
    delivery: &WorldContextAttestationV2<TaskContextPreparedDeliveryV2>,
) -> Result<ExactSignatureChecks> {
    let body = validate_fullsigned_prepared_storage_v2(scope.clone(), world, q, delivery)
        .map_err(|_| invalid())?;
    let mut jobs = prepared_jobs(q, &delivery.payload.private_body, &world.service_public_key)?;
    jobs.push(attestation(
        PREPARED_PROJECTION_DOMAIN_V2,
        &delivery.payload.projection,
        &world.service_public_key,
    )?);
    jobs.push(attestation(
        CONTEXT_PREPARE_ATTESTED_PATH_V2,
        delivery,
        &world.service_public_key,
    )?);
    bind(scope, &(q, delivery, body.canonical_bytes()), jobs)
}
pub fn fullsigned_captured_storage_checks_v2(
    scope: &ContextBodyScopeV2,
    world: &ApprovedWorldV1,
    prior: &AttestedSlotRefV2,
    q: &TaskContextGetAttestationRequestV2,
    delivery: &WorldContextAttestationV2<TaskContextCapturedDeliveryV2>,
) -> Result<ExactSignatureChecks> {
    let body = validate_fullsigned_captured_storage_v2(scope.clone(), world, prior, q, delivery)
        .map_err(|_| invalid())?;
    let AttestedContextEvidenceV2::Prepared {
        request: prepare,
        projection,
    } = &prior.evidence
    else {
        return Err(invalid());
    };
    let old = ws::SignedServiceResponse {
        request_digest: projection.payload.private_response.request_digest.clone(),
        payload: delivery.payload.private_body.payload.prepared.clone(),
        signature_hex: projection.payload.private_response.signature_hex.clone(),
    };
    let mut jobs = prepared_jobs(prepare, &old, &world.service_public_key)?;
    jobs.push(job(
        TASK_CONTEXT_READ_PATH,
        &q.original.request,
        &q.original.subject_public_key,
        &q.original.signature_hex,
    )?);
    let WorldServicePayloadV1::Scheduler(prefix) = &delivery
        .payload
        .private_body
        .payload
        .capture_proof
        .request
        .signed_payload
    else {
        return Err(invalid());
    };
    jobs.push(job(
        "scheduler",
        &prefix.request,
        &prefix.subject_public_key,
        &prefix.signature_hex,
    )?);
    jobs.push(response(
        TASK_CONTEXT_READ_PATH,
        &delivery.payload.private_body,
        &world.service_public_key,
    )?);
    jobs.push(attestation(
        PREPARED_PROJECTION_DOMAIN_V2,
        projection,
        &world.service_public_key,
    )?);
    jobs.push(attestation(
        CAPTURED_PROJECTION_DOMAIN_V2,
        &delivery.payload.projection,
        &world.service_public_key,
    )?);
    jobs.push(attestation(
        CONTEXT_READ_ATTESTED_PATH_V2,
        delivery,
        &world.service_public_key,
    )?);
    bind(scope, &(prior, q, delivery, body.canonical_bytes()), jobs)
}
/// Every final invocation reconstructs the complete manifest and SDK semantics.
/// This cannot turn an exact cache into a current/paid context permission.
pub fn validate_fullsigned_prepared_checked_v2(
    scope: ContextBodyScopeV2,
    world: &ApprovedWorldV1,
    q: &TaskContextPrepareAttestationRequestV2,
    delivery: &WorldContextAttestationV2<TaskContextPreparedDeliveryV2>,
    verifier: &mut impl ApprovalSignatureVerifier,
) -> core::result::Result<ValidatedFullSignedContextBodyV2, ContextBodyError> {
    let checks = fullsigned_prepared_storage_checks_v2(&scope, world, q, delivery)
        .map_err(|_| ContextBodyError::Invalid)?;
    verifier
        .verify(&checks)
        .map_err(|_| ContextBodyError::Invalid)?;
    validate_fullsigned_prepared_storage_v2(scope, world, q, delivery)
}
pub fn validate_fullsigned_captured_checked_v2(
    scope: ContextBodyScopeV2,
    world: &ApprovedWorldV1,
    prior: &AttestedSlotRefV2,
    q: &TaskContextGetAttestationRequestV2,
    delivery: &WorldContextAttestationV2<TaskContextCapturedDeliveryV2>,
    verifier: &mut impl ApprovalSignatureVerifier,
) -> core::result::Result<ValidatedFullSignedContextBodyV2, ContextBodyError> {
    let checks = fullsigned_captured_storage_checks_v2(&scope, world, prior, q, delivery)
        .map_err(|_| ContextBodyError::Invalid)?;
    verifier
        .verify(&checks)
        .map_err(|_| ContextBodyError::Invalid)?;
    validate_fullsigned_captured_storage_v2(scope, world, prior, q, delivery)
}
