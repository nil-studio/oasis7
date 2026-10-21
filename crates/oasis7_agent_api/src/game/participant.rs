//! Canonical first participant binding. Identity admission and owner consent are separate.
use super::DecimalU64;
use serde::{Deserialize, Serialize};
pub const GAME_PARTICIPANT_REGISTER_DOMAIN: &str = "game_participant_register_v1";
pub const GAME_PARTICIPANT_ADMISSION_DOMAIN: &str = "game_participant_admission_v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameIdentityNamespaceV1 {
    pub service_id: String,
    pub environment_id: String,
    pub instance_epoch: DecimalU64,
}
impl GameIdentityNamespaceV1 {
    pub fn same_authority(&self, other: &Self) -> bool {
        self.service_id == other.service_id && self.environment_id == other.environment_id
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameParticipantRegisterV1 {
    pub api_rev: u32,
    pub client_operation_id: String,
    pub identity_namespace: GameIdentityNamespaceV1,
    pub account_id: String,
    pub world_id: String,
    pub genesis_digest: String,
    pub owner_public_key: String,
    pub owner_key_version: DecimalU64,
}
impl GameParticipantRegisterV1 {
    pub fn validate(&self) -> Result<(), String> {
        if self.api_rev != 1
            || self.owner_key_version.0 != 1
            || self.identity_namespace.instance_epoch.0 == 0
            || [
                &self.client_operation_id,
                &self.identity_namespace.service_id,
                &self.identity_namespace.environment_id,
                &self.account_id,
                &self.world_id,
                &self.genesis_digest,
            ]
            .iter()
            .any(|s| s.trim().is_empty() || s.len() > 256)
            || self.owner_public_key.len() != 64
        {
            return Err("invalid participant registration".into());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameParticipantAdmissionV1 {
    pub api_rev: u32,
    pub business_digest: String,
    pub identity_namespace: GameIdentityNamespaceV1,
    pub account_id: String,
    pub world_id: String,
    pub genesis_digest: String,
    pub owner_public_key: String,
    pub branch_id: String,
    pub reorg_epoch: DecimalU64,
    pub policy_generation: DecimalU64,
    pub valid_from_tick: DecimalU64,
    pub valid_until_tick: DecimalU64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameParticipantReceiptV1 {
    pub api_rev: u32,
    pub identity_namespace: GameIdentityNamespaceV1,
    pub account_id: String,
    pub world_id: String,
    pub genesis_digest: String,
    pub agent_id: String,
    pub owner_public_key: String,
    pub owner_binding: String,
    pub owner_generation: DecimalU64,
    pub client_operation_id: String,
    pub business_digest: String,
    pub registered_tick: DecimalU64,
    pub imported: bool,
}
