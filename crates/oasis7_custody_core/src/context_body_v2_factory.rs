//! Identity-process-only storage validation. It never accepts an Account Current.
use super::*;
use oasis7_client_api::world_service::{
    SignedServiceResponse,
    task_context::{CapturedTaskContextV1, PreparedTaskContextV1},
    task_context_projection::*,
};
use zeroize::Zeroizing;

/// Only this factory can construct the opaque. It contains private bytes and
/// has no Deserialize/Serialize/Clone/Debug or context-use permission.
pub struct ValidatedFullSignedContextBodyV2 {
    scope: ContextBodyScopeV2,
    bytes: Zeroizing<Vec<u8>>,
}
impl ValidatedFullSignedContextBodyV2 {
    pub fn scope(&self) -> &ContextBodyScopeV2 {
        &self.scope
    }
    pub fn canonical_bytes(&self) -> &[u8] {
        self.bytes.as_slice()
    }
}
fn canonical<T: Serialize>(value: &T) -> ContextBodyV2Result<Zeroizing<Vec<u8>>> {
    let bytes = Zeroizing::new(
        oasis7_wasm_abi::encode_canonical_cbor(value).map_err(|_| ContextBodyError::Encoding)?,
    );
    if bytes.is_empty() || bytes.len() > MAX_ATTESTED_CONTEXT_BODY_BYTES_V2 {
        return Err(ContextBodyError::ContextTooLarge);
    }
    Ok(bytes)
}
fn wire<T: Serialize>(value: &T) -> ContextBodyV2Result<()> {
    let bytes = Zeroizing::new(serde_json::to_vec(value).map_err(|_| ContextBodyError::Encoding)?);
    if bytes.len() > MAX_ATTESTED_CONTEXT_WIRE_BYTES_V2 {
        return Err(ContextBodyError::ContextTooLarge);
    }
    Ok(())
}
fn common(scope: &ContextBodyScopeV2, trusted_world: &ApprovedWorldV1) -> ContextBodyV2Result<()> {
    scope.validate()?;
    if &scope.world != trusted_world || !trusted_world.valid() {
        return Err(ContextBodyError::Invalid);
    }
    Ok(())
}
fn body<T: Serialize>(
    scope: ContextBodyScopeV2,
    response: &SignedServiceResponse<T>,
) -> ContextBodyV2Result<ValidatedFullSignedContextBodyV2> {
    let bytes = canonical(response)?;
    let actual = ContextBodyCommitmentV2::from_signed_body(response)
        .map_err(|_| ContextBodyError::Invalid)?;
    if scope.body != actual {
        return Err(ContextBodyError::Conflict);
    }
    Ok(ValidatedFullSignedContextBodyV2 { scope, bytes })
}
/// The trusted caller loads the World pin independently of response/scope.
/// Successful validation is historical storage evidence, never paid dispatch.
pub fn validate_fullsigned_prepared_storage_v2(
    scope: ContextBodyScopeV2,
    trusted_world: &ApprovedWorldV1,
    request: &TaskContextPrepareAttestationRequestV2,
    delivery: &WorldContextAttestationV2<TaskContextPreparedDeliveryV2>,
) -> ContextBodyV2Result<ValidatedFullSignedContextBodyV2> {
    common(&scope, trusted_world)?;
    // Size is tested on the entire old-domain signed response, before any
    // conversion can discard signature metadata or the private payload.
    let _ = canonical(&delivery.payload.private_body)?;
    wire(delivery)?;
    verify_prepared_delivery_v2(request, delivery, &trusted_world.service_public_key)
        .map_err(|_| ContextBodyError::Invalid)?;
    let projection = &delivery.payload.projection;
    if scope.slot != ContextBodySlotKindV1::Prepared
        || scope.context != projection.payload.context
        || scope.prepare_operation_id != request.original.request.client_operation_id
        || scope.public_projection_digest
            != public_projection_digest_v2(projection).map_err(|_| ContextBodyError::Encoding)?
    {
        return Err(ContextBodyError::Conflict);
    }
    body(scope, &delivery.payload.private_body)
}
pub fn validate_fullsigned_captured_storage_v2(
    scope: ContextBodyScopeV2,
    trusted_world: &ApprovedWorldV1,
    prepared_ref: &AttestedSlotRefV2,
    request: &TaskContextGetAttestationRequestV2,
    delivery: &WorldContextAttestationV2<TaskContextCapturedDeliveryV2>,
) -> ContextBodyV2Result<ValidatedFullSignedContextBodyV2> {
    common(&scope, trusted_world)?;
    prepared_ref.validate_provenance(trusted_world)?;
    let AttestedContextEvidenceV2::Prepared {
        projection: original_projection,
        ..
    } = &prepared_ref.evidence
    else {
        return Err(ContextBodyError::Invalid);
    };
    let prior = &prepared_ref.scope;
    if prior.slot != ContextBodySlotKindV1::Prepared
        || prior.identity_scope != scope.identity_scope
        || prior.world != scope.world
        || prior.session_id != scope.session_id
        || prior.context_operation_id != scope.context_operation_id
        || prior.prepare_operation_id != scope.prepare_operation_id
        || prior.session_intent_digest != scope.session_intent_digest
        || prior.native_binding_digest != scope.native_binding_digest
        || prior.version != scope.version
        || prior.write_deadline != scope.write_deadline
        || prior.context != scope.context
        || &request.prepared_projection != original_projection.as_ref()
    {
        return Err(ContextBodyError::Conflict);
    }
    let _ = canonical(&delivery.payload.private_body)?;
    wire(delivery)?;
    verify_captured_delivery_v2(request, delivery, &trusted_world.service_public_key)
        .map_err(|_| ContextBodyError::Invalid)?;
    let projection = &delivery.payload.projection;
    if scope.slot != ContextBodySlotKindV1::Captured
        || scope.context != projection.payload.context
        || scope.prepare_operation_id != request.original.request.prepare_operation_id
        || scope.public_projection_digest
            != public_projection_digest_v2(projection).map_err(|_| ContextBodyError::Encoding)?
    {
        return Err(ContextBodyError::Conflict);
    }
    body(scope, &delivery.payload.private_body)
}

/// Account may receive only this public World attestation and sealed metadata.
/// No private-body signature is independently verified without its plaintext.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "slot", rename_all = "snake_case", deny_unknown_fields)]
pub enum AttestedContextEvidenceV2 {
    Prepared {
        request: Box<TaskContextPrepareAttestationRequestV2>,
        projection: Box<WorldContextAttestationV2<TaskContextPreparedProjectionV2>>,
    },
    Captured {
        request: Box<TaskContextGetAttestationRequestV2>,
        projection: Box<WorldContextAttestationV2<TaskContextCapturedProjectionV2>>,
    },
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttestedSlotRefV2 {
    pub api_version: u8,
    pub scope: ContextBodyScopeV2,
    pub evidence: AttestedContextEvidenceV2,
    pub envelope_digest: String,
    pub envelope_wire_bytes: DecimalU64,
}
impl core::fmt::Debug for AttestedSlotRefV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("AttestedSlotRefV2(<public proof; private body absent>)")
    }
}
impl AttestedSlotRefV2 {
    pub fn validate_provenance(&self, trusted_world: &ApprovedWorldV1) -> ContextBodyV2Result<()> {
        common(&self.scope, trusted_world)?;
        if self.api_version != 2
            || !hash(&self.envelope_digest)
            || self.envelope_wire_bytes.0 == 0
            || self.envelope_wire_bytes.0 > CONTEXT_BODY_MAX_WIRE_BYTES as u64
        {
            return Err(ContextBodyError::Invalid);
        }
        let (slot, context, prepare, projection_digest, commitment) = match &self.evidence {
            AttestedContextEvidenceV2::Prepared {
                request,
                projection,
            } => {
                verify_prepared_projection_v2(
                    request,
                    projection,
                    &trusted_world.service_public_key,
                )
                .map_err(|_| ContextBodyError::Invalid)?;
                (
                    ContextBodySlotKindV1::Prepared,
                    &projection.payload.context,
                    &request.original.request.client_operation_id,
                    public_projection_digest_v2(projection),
                    &projection.payload.body,
                )
            }
            AttestedContextEvidenceV2::Captured {
                request,
                projection,
            } => {
                verify_captured_projection_v2(
                    request,
                    projection,
                    &trusted_world.service_public_key,
                )
                .map_err(|_| ContextBodyError::Invalid)?;
                (
                    ContextBodySlotKindV1::Captured,
                    &projection.payload.context,
                    &request.original.request.prepare_operation_id,
                    public_projection_digest_v2(projection),
                    &projection.payload.body,
                )
            }
        };
        if self.scope.slot != slot
            || &self.scope.context != context
            || &self.scope.prepare_operation_id != prepare
            || self.scope.public_projection_digest
                != projection_digest.map_err(|_| ContextBodyError::Encoding)?
            || &self.scope.body != commitment
        {
            return Err(ContextBodyError::Conflict);
        }
        wire(self)
    }
    pub fn digest(&self, trusted_world: &ApprovedWorldV1) -> ContextBodyV2Result<String> {
        self.validate_provenance(trusted_world)?;
        Ok(bytes_digest(
            &oasis7_wasm_abi::encode_canonical_cbor(&("oasis7.custody.attested-slot-ref.v2", self))
                .map_err(|_| ContextBodyError::Encoding)?,
        ))
    }
}

/// Recovered bytes remain Identity-only. Canonical roundtrip and every SDK
/// signature/semantic check are repeated; storage metadata is not a verifier.
pub fn validate_recovered_fullsigned_storage_v2(
    reference: &AttestedSlotRefV2,
    trusted_world: &ApprovedWorldV1,
    bytes: &[u8],
) -> ContextBodyV2Result<ValidatedFullSignedContextBodyV2> {
    reference.validate_provenance(trusted_world)?;
    if bytes.is_empty() || bytes.len() > MAX_ATTESTED_CONTEXT_BODY_BYTES_V2 {
        return Err(ContextBodyError::ContextTooLarge);
    }
    fn decode<T: serde::de::DeserializeOwned + Serialize>(bytes: &[u8]) -> ContextBodyV2Result<T> {
        let value: T = serde_cbor::from_slice(bytes).map_err(|_| ContextBodyError::Encoding)?;
        if canonical(&value)?.as_slice() != bytes {
            return Err(ContextBodyError::Encoding);
        }
        Ok(value)
    }
    match &reference.evidence {
        AttestedContextEvidenceV2::Prepared {
            request,
            projection,
        } => {
            let response: SignedServiceResponse<PreparedTaskContextV1> = decode(bytes)?;
            let actual = derive_prepared_projection_v2(
                request,
                &response,
                &trusted_world.service_public_key,
            )
            .map_err(|_| ContextBodyError::Invalid)?;
            if actual != projection.payload {
                return Err(ContextBodyError::Conflict);
            }
            body(reference.scope.clone(), &response)
        }
        AttestedContextEvidenceV2::Captured {
            request,
            projection,
        } => {
            let response: SignedServiceResponse<CapturedTaskContextV1> = decode(bytes)?;
            let actual = derive_captured_projection_v2(
                request,
                &response,
                &trusted_world.service_public_key,
            )
            .map_err(|_| ContextBodyError::Invalid)?;
            if actual != projection.payload {
                return Err(ContextBodyError::Conflict);
            }
            body(reference.scope.clone(), &response)
        }
    }
}
impl ValidatedFullSignedContextBodyV2 {
    /// Record is the actual once-sealed original, not caller-provided metadata.
    pub fn reference(
        &self,
        evidence: AttestedContextEvidenceV2,
        record: &ContextBodyRecordV2,
        trusted_world: &ApprovedWorldV1,
    ) -> ContextBodyV2Result<AttestedSlotRefV2> {
        record.validate_for(&self.scope)?;
        let reference = AttestedSlotRefV2 {
            api_version: 2,
            scope: self.scope.clone(),
            evidence,
            envelope_digest: record.envelope_digest()?,
            envelope_wire_bytes: DecimalU64(record.wire_bytes()?.len() as u64),
        };
        reference.validate_provenance(trusted_world)?;
        Ok(reference)
    }
}
