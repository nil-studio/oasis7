//! Exact bootstrap rotation originals; Runtime owns source and authority checks.
use super::finality_rotation::{
    RotationRecord, deserialize_rotation_records, serialize_rotation_records,
};
use oasis7_agent_api::world_action::{
    Action, BOOTSTRAP_FINALITY_SOURCE_DOMAIN_V1, BootstrapFinalityKeyRotationBodyV1,
    BootstrapFinalityKeyRotationProofV1, BootstrapFinalityKeyRotationRoleV1,
    GovernanceAuthorizedActionV1,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Preserve the exact authorized action once, rather than duplicating its body
/// and proofs. Runtime verifies the original parent and signatures separately.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootstrapFinalityKeyRotationRecordV1 {
    pub authorized_action: GovernanceAuthorizedActionV1,
    pub applied_at_tick: crate::WorldTime,
}
impl BootstrapFinalityKeyRotationRecordV1 {
    pub fn typed_parts(
        &self,
    ) -> Result<
        (
            &BootstrapFinalityKeyRotationBodyV1,
            &BootstrapFinalityKeyRotationProofV1,
            &BootstrapFinalityKeyRotationProofV1,
        ),
        String,
    > {
        match self.authorized_action.action.as_ref() {
            Action::RotateGovernanceBootstrapFinalityKeyV1 {
                body,
                old_key_proof,
                new_key_proof,
                ..
            } => Ok((body, old_key_proof, new_key_proof)),
            _ => Err("rotation record does not retain a rotation action".into()),
        }
    }
    pub fn body(&self) -> Result<&BootstrapFinalityKeyRotationBodyV1, String> {
        self.typed_parts().map(|(body, _, _)| body)
    }
    pub fn validate(&self) -> Result<(), String> {
        let (body, old, new) = self.typed_parts()?;
        body.validate_authorization(&self.authorized_action.authorization)?;
        if self.applied_at_tick != body.decision_height {
            return Err("rotation record applied tick differs from decision height".into());
        }
        let Action::RotateGovernanceBootstrapFinalityKeyV1 {
            controller_account_id,
            ..
        } = self.authorized_action.action.as_ref()
        else {
            unreachable!("typed_parts checked rotation action")
        };
        if controller_account_id != &body.authorization.controller_account_id {
            return Err("rotation action controller differs from its body".into());
        }
        old.validate_for(
            body,
            BootstrapFinalityKeyRotationRoleV1::OldKeyAuthorization,
        )?;
        new.validate_for(body, BootstrapFinalityKeyRotationRoleV1::NewKeyPossession)?;
        Ok(())
    }
}

impl RotationRecord for BootstrapFinalityKeyRotationRecordV1 {
    fn validate_record(&self) -> Result<(), String> {
        self.validate()
    }
    fn rotation_id(&self) -> Result<&str, String> {
        self.body().map(|b| b.rotation_id.as_str())
    }
}
pub(crate) fn deserialize_bootstrap_finality_key_rotations<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<BTreeMap<String, BootstrapFinalityKeyRotationRecordV1>, D::Error> {
    deserialize_rotation_records(d)
}
pub(crate) struct BootstrapFinalityKeyRotationHistoryRef<'a>(
    pub &'a BTreeMap<String, BootstrapFinalityKeyRotationRecordV1>,
);
impl Serialize for BootstrapFinalityKeyRotationHistoryRef<'_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        serialize_rotation_records(self.0, s)
    }
}
/// Digest the original typed registry, never an effective or JSON projection.
/// Canonical bytes use fixed typed `serde_cbor::to_vec` field ordering, matching
/// the API preimage convention, without Value sorting or a self-describe tag.
pub fn bootstrap_finality_registry_digest_v1(
    registry: &super::GovernanceFinalitySignerRegistry,
) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let encoded = serde_cbor::to_vec(registry).map_err(|_| "bootstrap registry encoding")?;
    let domain = BOOTSTRAP_FINALITY_SOURCE_DOMAIN_V1.as_bytes();
    let mut hash = Sha256::new();
    hash.update((domain.len() as u64).to_be_bytes());
    hash.update(domain);
    hash.update((encoded.len() as u64).to_be_bytes());
    hash.update(&encoded);
    Ok(hex::encode(hash.finalize()))
}
#[cfg(test)]
#[path = "governance_bootstrap_finality_rotation_tests.rs"]
mod tests;
