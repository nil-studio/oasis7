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
