//! Nonsecret, nonce-bound current participant policy discovery.
use super::WorldIdentity;
use super::{ReadWorldViewRequest, SignedReadRequest};
use oasis7_agent_api::game::{GameParticipantAdmissionV1, GameParticipantRegisterV1};
use serde::{Deserialize, Serialize};
pub const PARTICIPANT_POLICY_PATH: &str = "/v1/world/participant-policy";
pub const PARTICIPANT_OWNER_VIEW_PATH: &str = "/v1/world/participant-owner-view";
pub const PARTICIPANT_STARTER_ELIGIBILITY_PATH: &str = "/v1/world/participant-starter-eligibility";
#[path = "participant_policy_original.rs"]
mod policy_original;
pub use policy_original::ParticipantPolicyOriginalV1;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantOwnerViewRequestV1 {
    pub owner: SignedReadRequest<GameParticipantRegisterV1>,
    pub admission: SignedReadRequest<GameParticipantAdmissionV1>,
    pub read: SignedReadRequest<ReadWorldViewRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_profile: Option<ParticipantOwnerViewProfileV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantPolicyRequestV1 {
    pub contract_version: u32,
    pub expected_world: WorldIdentity,
    pub trust_config_ref: String,
    pub observation_nonce: String,
}
impl ParticipantPolicyRequestV1 {
    pub fn validate(&self) -> Result<(), String> {
        self.expected_world.validate().map_err(|e| e.to_string())?;
        if self.contract_version != 1
            || self.trust_config_ref.len() != 64
            || self.observation_nonce.len() != 64
            || !self
                .observation_nonce
                .bytes()
                .all(|v| v.is_ascii_hexdigit())
        {
            return Err("invalid participant policy request".into());
        }
        Ok(())
    }
}

/// Explicit bounded control observation; absent profile retains the legacy view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantOwnerViewProfileV1 {
    OwnerControlV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantOwnerControlViewV1 {
    pub canonical_agent_owner: oasis7_proto::viewer::CanonicalAgentOwnerFenceV1,
    pub game_task_control: Option<oasis7_agent_api::game::GameTaskControlV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantOwnerControlResponseV1 {
    pub schema_revision: u32,
    pub world: WorldIdentity,
    pub version: super::ProjectionVersion,
    pub logical_tick: u64,
    pub view: ParticipantOwnerControlViewV1,
}
impl ParticipantOwnerControlResponseV1 {
    pub fn validate(&self, request: &ParticipantOwnerViewRequestV1) -> Result<(), String> {
        use super::authority::request_digest;
        request.read.request.validate().map_err(|e| e.to_string())?;
        self.version.commit.validate().map_err(|e| e.to_string())?;
        let owner = &self.view.canonical_agent_owner;
        let binding = &self.version.commit.binding;
        if request.response_profile != Some(ParticipantOwnerViewProfileV1::OwnerControlV1)
            || self.schema_revision != 1
            || self.world != request.read.request.world
            || self.version.commit.world != self.world
            || self.version.visibility_scope != request.read.request.scope_id
            || self.version.visibility_scope != format!("agent:{}", owner.agent_id)
            || request.read.request.fixed_commit.is_some()
            || request.read.request.formal_anchor.is_some()
            || request.read.request.observation_nonce.is_none()
            || self.version.projection_revision != request_digest("projection", &self.view)?
            || owner.world_id != self.world.world_id
            || owner.public_key != request.read.subject_public_key
            || owner.public_key != request.owner.subject_public_key
            || owner.canonical_authority.branch_id != binding.branch_id
            || owner.reorg_epoch != binding.reorg_generation
        {
            return Err("invalid participant owner control observation".into());
        }
        if let Some(minimum) = &request.read.request.min_commit
            && !self
                .version
                .commit
                .satisfies_minimum(minimum)
                .map_err(|e| e.to_string())?
        {
            return Err("participant owner control read not caught up".into());
        }
        if self.view.game_task_control.as_ref().is_some_and(|task| {
            task.schema_revision != 1
                || task.agent_id != owner.agent_id
                || task.world_id != self.world.world_id
                || task.genesis_digest != self.world.genesis_digest
                || task.owner_public_key != owner.public_key
                || task.owner_binding != owner.player_id
                || task.owner_generation.0 != owner.canonical_authority.agent_identity_generation
                || task.branch_id != binding.branch_id
                || task.reorg_epoch.0 != binding.reorg_generation
        }) {
            return Err("participant task differs from owner control observation".into());
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "participant_owner_control_tests.rs"]
mod owner_control_tests;
