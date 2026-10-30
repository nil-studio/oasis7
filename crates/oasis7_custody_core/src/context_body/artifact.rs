//! SDK-verified temporary body. No serialized verification or usable capability.
use super::*;
use oasis7_agent_api::game::{TaskContextAuthorityModeV1, TaskContextIdentityV1};
use oasis7_client_api::{
    world_service::{self as ws, task_context::*},
    world_service_wire::{SchedulerOperationV1, WorldServicePayloadV1},
};
use zeroize::Zeroizing;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextBodyResponseMaterialV1 {
    pub request_digest: String,
    pub signature_hex: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextPrefixProofV1 {
    pub original: ws::SubmitIntentRequest<WorldServicePayloadV1>,
    pub response: ws::SignedServiceResponse<ws::IntentResponse<serde_json::Value>>,
}
/// Only a validated closed public Prefix/canonical fact is encoded here, never a body.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextPrefixEvidenceV1 {
    canonical_cbor: Vec<u8>,
}
impl ContextPrefixProofV1 {
    pub(crate) fn validate(
        &self,
        scope: &ContextBodyScopeV1,
        identity: &TaskContextIdentityV1,
    ) -> Result<()> {
        self.original
            .validate()
            .map_err(|_| ContextBodyError::Invalid)?;
        let WorldServicePayloadV1::Scheduler(prefix) = &self.original.signed_payload else {
            return Err(ContextBodyError::Invalid);
        };
        let SchedulerOperationV1::TaskProviderPrefix { context } = &prefix.request.operation else {
            return Err(ContextBodyError::Invalid);
        };
        if context != identity
            || prefix.subject_public_key
                != identity
                    .fence
                    .executor_binding
                    .as_ref()
                    .ok_or(ContextBodyError::Invalid)?
                    .executor_public_key
            || self.original.deadline_unix_ms != Some(scope.write_deadline.0)
        {
            return Err(ContextBodyError::Invalid);
        }
        ws::private_goal::verify_owner_signature_strict(
            "scheduler",
            &prefix.request,
            &prefix.subject_public_key,
            &prefix.signature_hex,
        )
        .map_err(|_| ContextBodyError::Invalid)?;
        let expected =
            oasis7_client_api::world_service_correlation::derive_correlation_with_legacy(
                ws::WorldIdentity {
                    world_id: scope.world.world_id.clone(),
                    genesis_digest: scope.world.genesis_digest.clone(),
                },
                &self.original.signed_payload,
                |_| Err("context prefix is not legacy".into()),
            )
            .map_err(|_| ContextBodyError::Invalid)?;
        if expected != self.original.correlation {
            return Err(ContextBodyError::Invalid);
        }
        verify_private_reply(
            ws::SUBMIT_PATH,
            &self.original,
            &self.response,
            &scope.world.service_public_key,
        )
        .map_err(|_| ContextBodyError::Invalid)?;
        self.response
            .payload
            .validate(&self.original.correlation)
            .map_err(|_| ContextBodyError::Invalid)?;
        let ws::IntentOutcome::Committed { commit, .. } = &self.response.payload.outcome else {
            return Err(ContextBodyError::Invalid);
        };
        if commit.world.world_id != scope.world.world_id
            || commit.world.genesis_digest != scope.world.genesis_digest
            || commit.binding.branch_id != scope.world.branch_id
            || commit.binding.reorg_generation != scope.world.reorg_epoch.0
        {
            return Err(ContextBodyError::Invalid);
        }
        Ok(())
    }
    pub(crate) fn validate_against_prepared(
        &self,
        scope: &ContextBodyScopeV1,
        prepare: &ws::SignedReadRequest<TaskContextPrepareV1>,
        prepared: &PreparedTaskContextV1,
        original: &ws::SignedReadRequest<TaskContextGetV1>,
    ) -> Result<()> {
        verify_get(original).map_err(|_| ContextBodyError::Invalid)?;
        if original.request.context != prepared.identity
            || original.request.prepare_operation_id != prepare.request.client_operation_id
            || original.subject_public_key != prepare.subject_public_key
        {
            return Err(ContextBodyError::Invalid);
        }
        let expected = public_prefix(prepared).map_err(|_| ContextBodyError::Invalid)?;
        let WorldServicePayloadV1::Scheduler(signed) = &self.original.signed_payload else {
            return Err(ContextBodyError::Invalid);
        };
        if signed.request != expected {
            return Err(ContextBodyError::Invalid);
        }
        self.validate(scope, &original.request.context)
    }
    pub(crate) fn encode(
        &self,
        scope: &ContextBodyScopeV1,
        identity: &TaskContextIdentityV1,
    ) -> Result<ContextPrefixEvidenceV1> {
        self.validate(scope, identity)?;
        let canonical_cbor =
            oasis7_wasm_abi::encode_canonical_cbor(self).map_err(|_| ContextBodyError::Encoding)?;
        if canonical_cbor.len() > CONTEXT_BODY_MAX_BYTES {
            return Err(ContextBodyError::ContextTooLarge);
        }
        Ok(ContextPrefixEvidenceV1 { canonical_cbor })
    }
}
impl ContextPrefixEvidenceV1 {
    pub(crate) fn decode(&self) -> Result<ContextPrefixProofV1> {
        decode(&self.canonical_cbor)
    }
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ContextBodyEvidenceV1 {
    Prepared {
        original: ws::SignedReadRequest<TaskContextPrepareV1>,
        identity: TaskContextIdentityV1,
        response: ContextBodyResponseMaterialV1,
    },
    Captured {
        prepare: ws::SignedReadRequest<TaskContextPrepareV1>,
        original: Box<ws::SignedReadRequest<TaskContextGetV1>>,
        identity: TaskContextIdentityV1,
        prefix: ContextPrefixEvidenceV1,
        response: ContextBodyResponseMaterialV1,
    },
}
impl core::fmt::Debug for ContextBodyEvidenceV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ContextBodyEvidenceV1(<private-reference-not-authority>)")
    }
}
impl ContextBodyEvidenceV1 {
    pub fn slot(&self) -> ContextBodySlotKindV1 {
        match self {
            Self::Prepared { .. } => ContextBodySlotKindV1::Prepared,
            Self::Captured { .. } => ContextBodySlotKindV1::Captured,
        }
    }
    pub(crate) fn identity(&self) -> &TaskContextIdentityV1 {
        match self {
            Self::Prepared { identity, .. } | Self::Captured { identity, .. } => identity,
        }
    }
    pub(crate) fn prepare(&self) -> &ws::SignedReadRequest<TaskContextPrepareV1> {
        match self {
            Self::Prepared { original, .. } => original,
            Self::Captured { prepare, .. } => prepare,
        }
    }
    pub(crate) fn material(&self) -> &ContextBodyResponseMaterialV1 {
        match self {
            Self::Prepared { response, .. } | Self::Captured { response, .. } => response,
        }
    }
    /// Ref-only validation: deliberately cannot verify the missing response payload.
    pub(crate) fn validate_metadata(&self, scope: &ContextBodyScopeV1) -> Result<()> {
        scope.validate()?;
        verify_prepare(self.prepare()).map_err(|_| ContextBodyError::Invalid)?;
        self.identity()
            .validate()
            .map_err(|_| ContextBodyError::Invalid)?;
        let p = &self.prepare().request;
        if self.slot() != scope.slot
            || p.client_operation_id != scope.context_operation_id
            || p.authority_mode != Some(TaskContextAuthorityModeV1::NativeExecutorV1)
            || self.identity().authority_mode != p.authority_mode
            || self.identity().fence != p.fence
            || p.fence.world_id != scope.world.world_id
            || p.fence.genesis_digest != scope.world.genesis_digest
            || p.fence.branch_id != scope.world.branch_id
            || p.fence.reorg_epoch != scope.world.reorg_epoch
            || self.material().signature_hex.len() != 128
            || !self
                .material()
                .signature_hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(ContextBodyError::Invalid);
        }
        let expected = match self {
            Self::Prepared { original, .. } => {
                ws::request_digest(TASK_CONTEXT_PREPARE_PATH, original)
            }
            Self::Captured {
                original, prefix, ..
            } => {
                verify_get(original).map_err(|_| ContextBodyError::Invalid)?;
                if original.request.context != *self.identity()
                    || original.request.prepare_operation_id != p.client_operation_id
                {
                    return Err(ContextBodyError::Invalid);
                }
                prefix.decode()?.validate(scope, self.identity())?;
                ws::request_digest(TASK_CONTEXT_READ_PATH, original)
            }
        }
        .map_err(|_| ContextBodyError::Encoding)?;
        if expected != self.material().request_digest {
            return Err(ContextBodyError::Invalid);
        }
        Ok(())
    }
}
/// Private body and successful SDK validation cannot be deserialized, cloned or logged.
/// This is storage evidence, NOT permission to use/return context or sign a world write.
pub struct ValidatedContextBody {
    scope: ContextBodyScopeV1,
    evidence: ContextBodyEvidenceV1,
    body: Zeroizing<Vec<u8>>,
}
fn encode<T: Serialize>(value: &T) -> Result<Zeroizing<Vec<u8>>> {
    let bytes = Zeroizing::new(
        oasis7_wasm_abi::encode_canonical_cbor(value).map_err(|_| ContextBodyError::Encoding)?,
    );
    if bytes.is_empty() || bytes.len() > CONTEXT_BODY_MAX_BYTES {
        return Err(ContextBodyError::ContextTooLarge);
    }
    Ok(bytes)
}
fn decode<T: serde::de::DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<T> {
    if bytes.is_empty() || bytes.len() > CONTEXT_BODY_MAX_BYTES {
        return Err(ContextBodyError::ContextTooLarge);
    }
    let value: T = serde_cbor::from_slice(bytes).map_err(|_| ContextBodyError::Encoding)?;
    if encode(&value)?.as_slice() != bytes {
        return Err(ContextBodyError::Encoding);
    }
    Ok(value)
}
impl ValidatedContextBody {
    pub fn scope(&self) -> &ContextBodyScopeV1 {
        &self.scope
    }
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.body
    }
    pub(crate) fn evidence(&self) -> &ContextBodyEvidenceV1 {
        &self.evidence
    }
    pub(crate) fn prepared_response(
        &self,
    ) -> Result<ws::SignedServiceResponse<PreparedTaskContextV1>> {
        let ContextBodyEvidenceV1::Prepared { response, .. } = &self.evidence else {
            return Err(ContextBodyError::Invalid);
        };
        Ok(ws::SignedServiceResponse {
            request_digest: response.request_digest.clone(),
            signature_hex: response.signature_hex.clone(),
            payload: decode(&self.body)?,
        })
    }
    pub(crate) fn captured_response(
        &self,
    ) -> Result<ws::SignedServiceResponse<CapturedTaskContextV1>> {
        let ContextBodyEvidenceV1::Captured { response, .. } = &self.evidence else {
            return Err(ContextBodyError::Invalid);
        };
        Ok(ws::SignedServiceResponse {
            request_digest: response.request_digest.clone(),
            signature_hex: response.signature_hex.clone(),
            payload: decode(&self.body)?,
        })
    }
    pub(crate) fn prepared(&self) -> Result<PreparedTaskContextV1> {
        match self.scope.slot {
            ContextBodySlotKindV1::Prepared => decode(&self.body),
            ContextBodySlotKindV1::Captured => {
                Ok(decode::<CapturedTaskContextV1>(&self.body)?.prepared)
            }
        }
    }
    pub(crate) fn from_prepared(
        scope: ContextBodyScopeV1,
        original: &ws::SignedReadRequest<TaskContextPrepareV1>,
        response: &ws::SignedServiceResponse<PreparedTaskContextV1>,
    ) -> Result<Self> {
        verify_private_reply(
            TASK_CONTEXT_PREPARE_PATH,
            original,
            response,
            &scope.world.service_public_key,
        )
        .map_err(|_| ContextBodyError::Invalid)?;
        response
            .payload
            .validate_against(original)
            .map_err(|_| ContextBodyError::Invalid)?;
        let evidence = ContextBodyEvidenceV1::Prepared {
            original: original.clone(),
            identity: response.payload.identity.clone(),
            response: ContextBodyResponseMaterialV1 {
                request_digest: response.request_digest.clone(),
                signature_hex: response.signature_hex.clone(),
            },
        };
        evidence.validate_metadata(&scope)?;
        Ok(Self {
            scope,
            evidence,
            body: encode(&response.payload)?,
        })
    }
    pub(crate) fn from_captured(
        scope: ContextBodyScopeV1,
        prepare: &ws::SignedReadRequest<TaskContextPrepareV1>,
        original: &ws::SignedReadRequest<TaskContextGetV1>,
        prefix: &ContextPrefixEvidenceV1,
        response: &ws::SignedServiceResponse<CapturedTaskContextV1>,
    ) -> Result<Self> {
        verify_private_reply(
            TASK_CONTEXT_READ_PATH,
            original,
            response,
            &scope.world.service_public_key,
        )
        .map_err(|_| ContextBodyError::Invalid)?;
        response
            .payload
            .validate_against(original)
            .map_err(|_| ContextBodyError::Invalid)?;
        let fact = prefix.decode()?;
        let ws::IntentOutcome::Committed { commit, receipt } = &fact.response.payload.outcome
        else {
            return Err(ContextBodyError::Invalid);
        };
        let capture = &response.payload.capture_proof;
        if response.payload.prepared.prepare_proof != *prepare
            || capture.request != fact.original
            || capture.receipt != *receipt
            || capture.committed_height != commit.position
            || response.payload.capture_commit != **commit
        {
            return Err(ContextBodyError::Invalid);
        }
        let evidence = ContextBodyEvidenceV1::Captured {
            prepare: prepare.clone(),
            original: Box::new(original.clone()),
            identity: response.payload.prepared.identity.clone(),
            prefix: prefix.clone(),
            response: ContextBodyResponseMaterialV1 {
                request_digest: response.request_digest.clone(),
                signature_hex: response.signature_hex.clone(),
            },
        };
        evidence.validate_metadata(&scope)?;
        Ok(Self {
            scope,
            evidence,
            body: encode(&response.payload)?,
        })
    }
    pub(crate) fn revalidate(
        scope: ContextBodyScopeV1,
        evidence: &ContextBodyEvidenceV1,
        bytes: &[u8],
    ) -> Result<Self> {
        match evidence {
            ContextBodyEvidenceV1::Prepared {
                original, response, ..
            } => Self::from_prepared(
                scope,
                original,
                &ws::SignedServiceResponse {
                    request_digest: response.request_digest.clone(),
                    payload: decode(bytes)?,
                    signature_hex: response.signature_hex.clone(),
                },
            ),
            ContextBodyEvidenceV1::Captured {
                prepare,
                original,
                prefix,
                response,
                ..
            } => Self::from_captured(
                scope,
                prepare,
                original,
                prefix,
                &ws::SignedServiceResponse {
                    request_digest: response.request_digest.clone(),
                    payload: decode(bytes)?,
                    signature_hex: response.signature_hex.clone(),
                },
            ),
        }
    }
}
