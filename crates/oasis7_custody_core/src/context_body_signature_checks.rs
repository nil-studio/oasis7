//! Concrete Native context signature manifest. No caller verification token.
use super::*;
use crate::context_body::{ContextBodyEvidenceV1, ContextBodyScopeV1, ContextPrefixProofV1};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use oasis7_agent_api::game::GAME_GOAL_PREPARE_DOMAIN;
use oasis7_client_api::{
    world_service::{self as ws, task_context::*},
    world_service_wire::WorldServicePayloadV1,
};

pub(crate) enum ContextSignatureFact<'a> {
    Prepare(&'a ws::SignedReadRequest<TaskContextPrepareV1>),
    Prepared {
        original: &'a ws::SignedReadRequest<TaskContextPrepareV1>,
        response: &'a ws::SignedServiceResponse<PreparedTaskContextV1>,
        world_key: &'a str,
    },
    Get {
        original: &'a ws::SignedReadRequest<TaskContextGetV1>,
        prefix: &'a ContextPrefixProofV1,
        world_key: &'a str,
    },
    LiveCaptured {
        request: &'a TaskContextLiveReadV1,
        response: &'a ws::SignedServiceResponse<CapturedTaskContextV1>,
        world_key: &'a str,
    },
    Captured {
        response: &'a ws::SignedServiceResponse<CapturedTaskContextV1>,
        world_key: &'a str,
    },
}
fn job<T: Serialize>(
    domain: &str,
    value: &T,
    key: &str,
    signature: &str,
) -> Result<SignatureCheck> {
    let prepared = ws::private_goal::prepare_owner_signature_strict(domain, value, key, signature)
        .map_err(|_| ApprovalAdmissionError::Invalid)?;
    let check = SignatureCheck {
        key_id: format!("context-body:{domain}"),
        public_key_hex: key.into(),
        signature_base64url: URL_SAFE_NO_PAD.encode(prepared.signature_bytes()),
        canonical_bytes: prepared.canonical_bytes().to_vec(),
    };
    // These are exactly the bytes handed to the asynchronous adapter, not a
    // separate handwritten encoder or a serialized assertion of success.
    prepared
        .verify_strict()
        .map_err(|_| ApprovalAdmissionError::SignatureRejected)?;
    Ok(check)
}
pub(crate) fn append_context_signature_checks(
    mut checks: ExactSignatureChecks,
    fact: ContextSignatureFact<'_>,
) -> Result<ExactSignatureChecks> {
    let jobs = match fact {
        ContextSignatureFact::Prepare(v) => vec![job(
            TASK_CONTEXT_PREPARE_PATH,
            &v.request,
            &v.subject_public_key,
            &v.signature_hex,
        )?],
        ContextSignatureFact::Prepared {
            original,
            response,
            world_key,
        } => {
            let owner = &response.payload.snapshot.active_goal.owner_proof;
            vec![
                job(
                    TASK_CONTEXT_PREPARE_PATH,
                    &original.request,
                    &original.subject_public_key,
                    &original.signature_hex,
                )?,
                job(
                    GAME_GOAL_PREPARE_DOMAIN,
                    &owner.request,
                    &owner.subject_public_key,
                    &owner.signature_hex,
                )?,
                job(
                    TASK_CONTEXT_PREPARE_PATH,
                    &(&response.request_digest, &response.payload),
                    world_key,
                    &response.signature_hex,
                )?,
            ]
        }
        ContextSignatureFact::Get {
            original,
            prefix,
            world_key,
        } => {
            let WorldServicePayloadV1::Scheduler(signed) = &prefix.original.signed_payload else {
                return Err(ApprovalAdmissionError::Invalid);
            };
            vec![
                job(
                    TASK_CONTEXT_READ_PATH,
                    &original.request,
                    &original.subject_public_key,
                    &original.signature_hex,
                )?,
                job(
                    "scheduler",
                    &signed.request,
                    &signed.subject_public_key,
                    &signed.signature_hex,
                )?,
                job(
                    ws::SUBMIT_PATH,
                    &(&prefix.response.request_digest, &prefix.response.payload),
                    world_key,
                    &prefix.response.signature_hex,
                )?,
            ]
        }
        ContextSignatureFact::LiveCaptured {
            request,
            response,
            world_key,
        } => {
            verify_live_context_reply(request, response, world_key)
                .map_err(|_| ApprovalAdmissionError::Invalid)?;
            vec![job(
                TASK_CONTEXT_LIVE_READ_PATH,
                &(&response.request_digest, &response.payload),
                world_key,
                &response.signature_hex,
            )?]
        }
        ContextSignatureFact::Captured {
            response,
            world_key,
        } => vec![job(
            TASK_CONTEXT_READ_PATH,
            &(&response.request_digest, &response.payload),
            world_key,
            &response.signature_hex,
        )?],
    };
    checks.checks.extend(jobs);
    checks.body_digest = canonical_digest(
        "oasis7.custody.context-body.signature-jobs.v1",
        &(&checks.body_digest, &checks.checks),
    )?;
    Ok(checks)
}
pub(crate) fn bind_context_body_checks(
    mut checks: ExactSignatureChecks,
    scope: &ContextBodyScopeV1,
    evidence: &ContextBodyEvidenceV1,
    bytes: &[u8],
) -> Result<ExactSignatureChecks> {
    checks.body_digest = canonical_digest(
        "oasis7.custody.context-body.complete-artifact.v1",
        &(&checks.body_digest, scope, evidence, bytes),
    )?;
    Ok(checks)
}

pub(crate) fn bind_context_reference_checks(
    mut checks: ExactSignatureChecks,
    reference: &crate::provisioning::ContextBodyReferenceV1,
) -> Result<ExactSignatureChecks> {
    checks.body_digest = canonical_digest(
        "oasis7.custody.context-body.fixed-read-reference.v1",
        &(&checks.body_digest, reference),
    )?;
    Ok(checks)
}

/// A historical manifest never supplies live authority or a dispatch permit.
pub(crate) fn context_historical_checks(
    operation: &str,
    reference: &crate::provisioning::ContextBodyReferenceV1,
) -> Result<ExactSignatureChecks> {
    reference
        .scope
        .validate()
        .map_err(|_| ApprovalAdmissionError::Invalid)?;
    reference
        .evidence
        .validate_metadata(&reference.scope)
        .map_err(|_| ApprovalAdmissionError::Invalid)?;
    let binding = canonical_digest("oasis7.custody.context-history.v1", reference)?;
    Ok(ExactSignatureChecks {
        operation_id: operation.into(),
        body_digest: binding.clone(),
        context_digest: binding.clone(),
        verify_nonce: binding,
        verification_digest: None,
        checks: Vec::new(),
    })
}
#[derive(Serialize)]
pub(crate) struct LiveContextCurrentBinding<'a> {
    pub source: &'a str,
    pub ticket: u8,
    pub observed_at: crate::time_units::UnixMillis,
    pub private_generation: crate::DecimalU64,
    pub lifetime_digest: &'a str,
}
pub(crate) fn bind_live_context_checks(
    mut checks: ExactSignatureChecks,
    record: &crate::provisioning::LiveContextReadRecordV1,
    fact: &crate::provisioning::LiveContextReadFactV1,
    bytes: &[u8],
    current: Option<&LiveContextCurrentBinding<'_>>,
) -> Result<ExactSignatureChecks> {
    checks.body_digest = canonical_digest(
        "oasis7.custody.live-context.complete.v1",
        &(&checks.body_digest, record, fact, bytes, current),
    )?;
    Ok(checks)
}

/// Only historical storage material. This manifest carries no Current or grant.
pub(crate) fn context_storage_checks(
    body: &crate::context_body::ValidatedContextBody,
) -> Result<ExactSignatureChecks> {
    let binding = canonical_digest(
        "oasis7.custody.context-body.storage-producer.v1",
        &(body.scope(), body.evidence(), body.canonical_bytes()),
    )?;
    Ok(ExactSignatureChecks {
        operation_id: body.scope().context_operation_id.clone(),
        body_digest: binding.clone(),
        context_digest: binding.clone(),
        verify_nonce: binding,
        verification_digest: None,
        checks: Vec::new(),
    })
}
