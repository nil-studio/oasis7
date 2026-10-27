//! Explicit setup profile. The existing OwnerControlV1 wire remains unchanged.
use super::*;
use crate::world_service_wire::AgentSignerDelegationChangeV1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSetupObservationV1 {
    pub api_version: u8,
    pub control: ParticipantOwnerControlResponseV1,
    /// Current canonical per-Agent record, including revocation/high-water facts.
    /// None is an authenticated absence only within this signed complete profile.
    pub signer_delegation: Option<AgentSignerDelegationChangeV1>,
}
impl OwnerSetupObservationV1 {
    pub fn validate(&self, request: &ParticipantOwnerViewRequestV1) -> Result<(), String> {
        if self.api_version != 1
            || request.response_profile != Some(ParticipantOwnerViewProfileV1::OwnerSetupV1)
        {
            return Err("invalid owner setup observation profile".into());
        }
        let mut inner = request.clone();
        inner.response_profile = Some(ParticipantOwnerViewProfileV1::OwnerControlV1);
        self.control.validate(&inner)?;
        if let Some(delegation) = &self.signer_delegation {
            let owner = &self.control.view.canonical_agent_owner;
            if delegation.world_id != self.control.world.world_id
                || delegation.branch_id != owner.canonical_authority.branch_id
                || delegation.agent_id != owner.agent_id
                || delegation.owner_binding != owner.player_id
                || delegation.agent_identity_generation
                    != owner.canonical_authority.agent_identity_generation
                || delegation.generation == 0
                || delegation.nonce == 0
                || delegation.delegate_public_key.len() != 64
                || !delegation
                    .delegate_public_key
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err("owner setup delegation differs from canonical owner".into());
            }
        }
        Ok(())
    }
}
