//! Exact public Prepare signatures. This data never grants Current or a send.
use super::*;
use oasis7_client_api::world_service::{
    self as ws,
    entry_executor::EntryExecutorAccessV1,
    private_goal::verify_owner_signature_strict,
    task_context_projection::{
        TaskContextPrepareAttestationRequestV2, validate_prepare_attestation_request_v2,
    },
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionPrepareSignedOriginalV2 {
    pub access: EntryExecutorAccessV1,
    pub request: TaskContextPrepareAttestationRequestV2,
}
impl ExecutionPrepareSignedOriginalV2 {
    /// Exact existing public-original commitment; this does not verify a
    /// signer, Current, payment or dispatch permission.
    pub fn commitment_digest(&self) -> core::result::Result<String, String> {
        oasis7_wasm_abi::encode_canonical_cbor(self)
            .map(|bytes| crate::approval::bytes_digest(&bytes))
            .map_err(|e| e.to_string())
    }
}
impl PartialEq for ExecutionPrepareSignedOriginalV2 {
    fn eq(&self, other: &Self) -> bool {
        match (
            oasis7_wasm_abi::encode_canonical_cbor(self),
            oasis7_wasm_abi::encode_canonical_cbor(other),
        ) {
            (Ok(left), Ok(right)) => left == right,
            _ => false,
        }
    }
}
impl core::fmt::Debug for ExecutionPrepareSignedOriginalV2 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ExecutionPrepareSignedOriginalV2(<unverified-public-original>)")
    }
}
impl ExecutionSigningBundleV2 {
    pub fn verify_prepare_original(&self, signed: &ExecutionPrepareSignedOriginalV2) -> Result<()> {
        let invalid = || ProvisionApprovalError::BindingMismatch;
        let ExecutionSigningActionV2::Prepare {
            original,
            observation_nonce,
        } = self.action.as_ref()
        else {
            return Err(invalid());
        };
        let executor = &self
            .fence
            .executor_binding
            .as_ref()
            .ok_or_else(invalid)?
            .executor_public_key;
        if self.api_version != 2
            || !self.world.valid()
            || self.fence != original.fence
            || self.fence != signed.access.fence
            || signed.access.read.request != self.observation
            || signed.access.read.subject_public_key != *executor
            || signed.request.original.subject_public_key != *executor
            || signed.request.original.request != **original
            || signed.request.observation_nonce != *observation_nonce
            || original.authority_mode
                != Some(oasis7_agent_api::game::TaskContextAuthorityModeV1::NativeExecutorV1)
            || self.observation.fixed_commit.is_some()
            || self.observation.observation_nonce.is_none()
            || self.observation.scope_id != format!("agent:{}", self.fence.agent_id)
            || self.observation.world.world_id != self.world.world_id
            || self.observation.world.genesis_digest != self.world.genesis_digest
            || self.observation.deadline_unix_ms != Some(self.execution_deadline.get())
            || executor == &self.fence.owner_public_key
        {
            return Err(invalid());
        }
        self.fence.validate().map_err(|_| invalid())?;
        self.observation.validate().map_err(|_| invalid())?;
        verify_owner_signature_strict(
            ws::VIEW_JSON_PATH,
            &self.observation,
            executor,
            &signed.access.read.signature_hex,
        )
        .map_err(|_| invalid())?;
        validate_prepare_attestation_request_v2(&signed.request).map_err(|_| invalid())
    }
}
