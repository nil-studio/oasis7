//! Portable original qualified-H identity, not a verified successor capability.
//! Fields and origin framing preserve successor WIP 1d9e9d574 exactly.
use super::formal_handoff::{canonical_bytes, domain_digest};
use oasis7_proto::distributed_bft::v2::BftScopeV2;
use serde::{Deserialize, Serialize};

fn invalid(message: &str) -> String {
    message.to_owned()
}

pub const BFT_SUCCESSOR_WINDOW_ORIGIN_DOMAIN_V1: &str = "oasis7.bft.successor-window.v1.origin";

fn hash(value: &str) -> Result<(), String> {
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
fn identity(value: &str) -> Result<(), String> {
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
    pub fn validate(&self) -> Result<(), String> {
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
    pub fn origin_digest(&self) -> Result<String, String> {
        self.validate()?;
        Ok(domain_digest(
            BFT_SUCCESSOR_WINDOW_ORIGIN_DOMAIN_V1,
            &canonical_bytes(self, 4096, "successor original source")?,
        ))
    }
}

#[cfg(test)]
#[path = "successor_source_tests.rs"]
mod tests;
