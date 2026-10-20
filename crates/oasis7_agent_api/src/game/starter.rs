//! Owner-only starter claim; amounts and funding sources are exclusively world rules.
use super::DecimalU64;
use serde::{Deserialize, Serialize};
pub const GAME_STARTER_CLAIM_DOMAIN: &str = "game_starter_claim_v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameStarterClaimV1 {
    pub api_rev: u32,
    pub client_operation_id: String,
    pub world_id: String,
    pub genesis_digest: String,
    pub branch_id: String,
    pub reorg_epoch: DecimalU64,
    pub agent_id: String,
    pub owner_binding: String,
    pub owner_generation: DecimalU64,
}
impl GameStarterClaimV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.api_rev != 1
            || self.owner_generation.0 == 0
            || [
                &self.client_operation_id,
                &self.world_id,
                &self.genesis_digest,
                &self.branch_id,
                &self.agent_id,
                &self.owner_binding,
            ]
            .iter()
            .any(|s| s.trim().is_empty() || s.len() > 256)
        {
            return Err("invalid starter claim request".into());
        }
        Ok(())
    }
}
