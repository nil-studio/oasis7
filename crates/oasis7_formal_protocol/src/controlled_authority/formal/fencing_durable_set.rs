//! Provider transition commands are separate from ordinary initialization.
//! Unsupported adapters fail closed. Installed is NOT active signing authority.
use super::*;
use crate::controlled_authority::formal::{
    DurableSetInstallReceiptV1, DurableSetSealedEvidenceV1, FormalQualifiedCommitProofV1,
};
use serde::{Deserialize, Serialize};

/// Checkpoint-bound lifecycle marker. Presence without activation is staging
/// only; it never grants receipt signing or ordinary checkpoint advancement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DurableSetNativeBindingV1 {
    pub transition_digest: String,
    pub independent_inventory_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activation_digest: Option<String>,
}
impl DurableSetNativeBindingV1 {
    pub fn validate(&self) -> Result<(), FencingError> {
        self.validate_with_work(&mut |_| Ok::<(), FencingError>(()))
    }
    pub fn validate_with_work<E: From<FencingError>>(
        &self,
        work: &mut impl FnMut(usize) -> Result<(), E>,
    ) -> Result<(), E> {
        work(self.transition_digest.len())?;
        canonical_hash(&self.transition_digest)?;
        work(self.independent_inventory_digest.len())?;
        canonical_hash(&self.independent_inventory_digest)?;
        if let Some(digest) = &self.activation_digest {
            work(digest.len())?;
            canonical_hash(digest)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DurableSetNativeInstallRequestV1 {
    pub schema_version: u32,
    pub authorization: DurableSetSealedEvidenceV1,
    pub predecessor_proof: FormalQualifiedCommitProofV1,
    pub boundary_proof: FormalQualifiedCommitProofV1,
    pub source_checkpoint_bytes: Vec<u8>,
    /// Independently verified local object closure descriptor; the provider
    /// must read-back its own registered storage, never accept this assertion.
    pub local_closure_manifest_digest: String,
    pub registration: ProtectedEndpointRegistrationV1,
    /// P must consume its exact old sealed floor. New R has no old floor.
    pub expected_old_checkpoint: Option<EndpointCheckpointBindingV1>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DurableSetNativeActivationRequestV1 {
    pub schema_version: u32,
    pub install: DurableSetNativeInstallRequestV1,
    pub primary_install: DurableSetInstallReceiptV1,
    pub replica_install: DurableSetInstallReceiptV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DurableSetProviderStateV1 {
    pub transition_digest: String,
    pub successor_authority_id: String,
    pub install_receipt: DurableSetInstallReceiptV1,
    /// None means installed but mutation signing is forbidden. Some is the
    /// complete dual-install activation digest independently checked by provider.
    pub activation_digest: Option<String>,
}
impl DurableSetNativeInstallRequestV1 {
    pub fn validate_bounds(&self) -> Result<(), FencingError> {
        if self.schema_version != 1
            || self.source_checkpoint_bytes.len() > 64 * 1024
            || self.registration.unsigned_checkpoint_preimage.len() > 64 * 1024
            || self
                .registration
                .existing_checkpoint
                .as_ref()
                .is_some_and(|b| b.len() > 64 * 1024)
        {
            return Err(FencingError::FencingUnproven);
        }
        canonical_hash(&self.local_closure_manifest_digest)?;
        canonical_hash(&self.registration.authority_id)?;
        canonical_hash(&self.registration.activation_ref)?;
        // Framed, finite per-request encoding, not a lifetime history cap.
        if serde_cbor::to_vec(self)
            .map_err(|e| invalid(e.to_string()))?
            .len()
            > 1024 * 1024
        {
            return Err(FencingError::FencingUnproven);
        }
        Ok(())
    }
    pub fn transition_digest(&self) -> Result<String, FencingError> {
        self.authorization
            .transition
            .body
            .digest()
            .map_err(|e| invalid(e.to_string()))
    }
}
