//! Stateless historical storage producers. No Account, Current or dispatch authority.
use super::*;
use crate::approval_admission::{
    ApprovalAdmissionError, ApprovalSignatureVerifier, ContextSignatureFact, ExactSignatureChecks,
    append_context_signature_checks, bind_context_body_checks, context_storage_checks,
};
use oasis7_client_api::world_service::{self as ws, task_context::*};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextStorageError {
    Artifact(ContextBodyError),
    Signature(ApprovalAdmissionError),
}
impl From<ContextBodyError> for ContextStorageError {
    fn from(value: ContextBodyError) -> Self {
        Self::Artifact(value)
    }
}
impl From<ApprovalAdmissionError> for ContextStorageError {
    fn from(value: ApprovalAdmissionError) -> Self {
        Self::Signature(value)
    }
}
type StorageResult<T> = core::result::Result<T, ContextStorageError>;
fn invalid() -> ContextStorageError {
    ContextBodyError::Invalid.into()
}

/// Borrowed in-process inputs, never a serialized verification or permission.
/// The fixed Identity caller supplies `trusted_world` from deployment pins,
/// independently of the scope/request. Core cannot prove that configuration I/O.
pub struct PreparedContextStorageInput<'a> {
    pub scope: &'a ContextBodyScopeV1,
    pub trusted_world: &'a ApprovedWorldV1,
    pub original: &'a ws::SignedReadRequest<TaskContextPrepareV1>,
    pub response: &'a ws::SignedServiceResponse<PreparedTaskContextV1>,
}
pub struct CapturedContextStorageInput<'a> {
    pub scope: &'a ContextBodyScopeV1,
    pub prepared: PreparedContextStorageInput<'a>,
    pub original: &'a ws::SignedReadRequest<TaskContextGetV1>,
    pub prefix: &'a ContextPrefixProofV1,
    pub response: &'a ws::SignedServiceResponse<CapturedTaskContextV1>,
}
fn prepared_material(
    input: &PreparedContextStorageInput<'_>,
) -> StorageResult<(ValidatedContextBody, ExactSignatureChecks)> {
    if input.scope.slot != ContextBodySlotKindV1::Prepared
        || input.scope.world != *input.trusted_world
        || !input.trusted_world.valid()
    {
        return Err(invalid());
    }
    let body =
        ValidatedContextBody::from_prepared(input.scope.clone(), input.original, input.response)?;
    let checks = append_context_signature_checks(
        context_storage_checks(&body)?,
        ContextSignatureFact::Prepared {
            original: input.original,
            response: input.response,
            world_key: &input.trusted_world.service_public_key,
        },
    )?;
    let checks = bind_context_body_checks(
        checks,
        body.scope(),
        body.evidence(),
        body.canonical_bytes(),
    )?;
    Ok((body, checks))
}
fn captured_material(
    input: &CapturedContextStorageInput<'_>,
) -> StorageResult<(ValidatedContextBody, ExactSignatureChecks)> {
    let mut prepared_scope = input.scope.clone();
    prepared_scope.slot = ContextBodySlotKindV1::Prepared;
    if input.scope.slot != ContextBodySlotKindV1::Captured
        || &prepared_scope != input.prepared.scope
        || input.scope.world != *input.prepared.trusted_world
    {
        return Err(invalid());
    }
    let (prepared, checks) = prepared_material(&input.prepared)?;
    let bytes = zeroize::Zeroizing::new(
        oasis7_wasm_abi::encode_canonical_cbor(&input.response.payload.prepared)
            .map_err(|_| ContextBodyError::Encoding)?,
    );
    if prepared.canonical_bytes() != bytes.as_slice() {
        return Err(invalid());
    }
    input.prefix.validate_against_prepared(
        input.scope,
        input.prepared.original,
        &input.prepared.response.payload,
        input.original,
    )?;
    let prefix = input
        .prefix
        .encode(input.scope, &input.original.request.context)?;
    let body = ValidatedContextBody::from_captured(
        input.scope.clone(),
        input.prepared.original,
        input.original,
        &prefix,
        input.response,
    )?;
    let checks = append_context_signature_checks(
        checks,
        ContextSignatureFact::Get {
            original: input.original,
            prefix: input.prefix,
            world_key: &input.prepared.trusted_world.service_public_key,
        },
    )?;
    let checks = append_context_signature_checks(
        checks,
        ContextSignatureFact::Captured {
            response: input.response,
            world_key: &input.prepared.trusted_world.service_public_key,
        },
    )?;
    let checks = bind_context_body_checks(
        checks,
        body.scope(),
        body.evidence(),
        body.canonical_bytes(),
    )?;
    Ok((body, checks))
}
/// Reconstructs all three exact SDK jobs; success does not return a private body.
pub fn prepared_context_storage_checks(
    input: &PreparedContextStorageInput<'_>,
) -> StorageResult<ExactSignatureChecks> {
    Ok(prepared_material(input)?.1)
}
/// Reconstructs all seven jobs including the original World Prepared signature.
pub fn captured_context_storage_checks(
    input: &CapturedContextStorageInput<'_>,
) -> StorageResult<ExactSignatureChecks> {
    Ok(captured_material(input)?.1)
}
/// After async WebCrypto the caller supplies its exact-manifest cache. Every
/// SDK semantic/strict check is rebuilt before any storage-only opaque is returned.
pub fn validate_prepared_context_storage(
    input: &PreparedContextStorageInput<'_>,
    verifier: &mut impl ApprovalSignatureVerifier,
) -> StorageResult<ValidatedContextBody> {
    let (body, checks) = prepared_material(input)?;
    verifier.verify(&checks)?;
    Ok(body)
}
pub fn validate_captured_context_storage(
    input: &CapturedContextStorageInput<'_>,
    verifier: &mut impl ApprovalSignatureVerifier,
) -> StorageResult<ValidatedContextBody> {
    let (body, checks) = captured_material(input)?;
    verifier.verify(&checks)?;
    Ok(body)
}
