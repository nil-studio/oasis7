//! Nonsecret, nonce-bound current participant policy discovery.
use super::WorldIdentity;
use super::{ReadWorldViewRequest, SignedReadRequest};
use oasis7_agent_api::game::{GameParticipantAdmissionV1, GameParticipantRegisterV1};
use serde::{Deserialize, Serialize};
pub const PARTICIPANT_POLICY_PATH: &str = "/v1/world/participant-policy";
pub const PARTICIPANT_OWNER_VIEW_PATH: &str = "/v1/world/participant-owner-view";
pub const PARTICIPANT_STARTER_ELIGIBILITY_PATH: &str = "/v1/world/participant-starter-eligibility";
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticipantOwnerViewRequestV1 {
    pub owner: SignedReadRequest<GameParticipantRegisterV1>,
    pub admission: SignedReadRequest<GameParticipantAdmissionV1>,
    pub read: SignedReadRequest<ReadWorldViewRequest>,
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
