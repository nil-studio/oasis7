//! Original qualified-H successor identity; shape validation is not authority.
//! Extracted unchanged from successor WIP 1d9e9d574dbf52cb81bf5f218e83179516972d4a.
//! This does not install a checkpoint, grant signing, or validate a successor chain.
use super::binding::{BftSignerBindingV3, BftSignerErrorV3, invalid};
use super::crypto::digest_hex;
use oasis7_proto::distributed_bft::v2::BftScopeV2;
use serde::{Deserialize, Serialize};

pub const BFT_SUCCESSOR_WINDOW_ORIGIN_DOMAIN_V1: &str = "oasis7.bft.successor-window.v1.origin";

fn hash(value: &str) -> Result<(), BftSignerErrorV3> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid(
            "successor window requires a lowercase 32-byte hash",
        ));
    }
    Ok(())
}
fn identity(value: &str) -> Result<(), BftSignerErrorV3> {
    if value.is_empty() || value.len() > 256 || value.trim() != value {
        return Err(invalid("successor window identity bounds"));
    }
    Ok(())
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BftSuccessorWindowSourceV1 {
    pub scope: BftScopeV2,
    pub binding_digest: String,
    pub handoff_position: u64,
    pub handoff_record_id: String,
    pub handoff_snapshot_root: String,
    pub target_digest: String,
    pub activation_digest: String,
    pub material_identity: String,
}
impl BftSuccessorWindowSourceV1 {
    pub fn from_binding(binding: &BftSignerBindingV3) -> Result<Self, BftSignerErrorV3> {
        binding.validate()?;
        Ok(Self {
            scope: binding.scope.clone(),
            binding_digest: binding.digest()?,
            handoff_position: binding.handoff_position,
            handoff_record_id: binding.handoff_record_id.clone(),
            handoff_snapshot_root: binding.handoff().raw_snapshot_root().into(),
            target_digest: binding.handoff().target_digest().into(),
            activation_digest: binding.activation_digest.clone(),
            material_identity: binding.material_identity.clone(),
        })
    }
    pub fn validate(&self) -> Result<(), BftSignerErrorV3> {
        identity(&self.scope.world_id)?;
        identity(&self.scope.chain_id)?;
        for value in [
            &self.scope.genesis_hash,
            &self.binding_digest,
            &self.handoff_record_id,
            &self.handoff_snapshot_root,
            &self.target_digest,
            &self.activation_digest,
            &self.material_identity,
        ] {
            hash(value)?;
        }
        if self.handoff_position == 0 || self.handoff_position.checked_add(1).is_none() {
            return Err(invalid("successor H position"));
        }
        Ok(())
    }
    pub fn validate_for_binding(
        &self,
        binding: &BftSignerBindingV3,
    ) -> Result<(), BftSignerErrorV3> {
        self.validate()?;
        if self != &Self::from_binding(binding)? {
            return Err(invalid(
                "successor source differs from original qualified H/binding",
            ));
        }
        Ok(())
    }
    pub fn origin_digest(&self) -> Result<String, BftSignerErrorV3> {
        self.validate()?;
        digest_hex(BFT_SUCCESSOR_WINDOW_ORIGIN_DOMAIN_V1, self)
    }
}

#[cfg(test)]
#[path = "successor_source_tests.rs"]
mod tests;
