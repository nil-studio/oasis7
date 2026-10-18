//! Cohesive prospective state-root helpers shared by tick publication paths.

use super::super::state::WorldStateProjection;
use super::super::util::hash_json;
use super::super::{GovernanceIdentityProfileState, WorldError, WorldState};
use super::World;
use super::tick_consensus::StateRootProjection;

impl World {
    pub(crate) fn current_state_root_hash(&self) -> Result<String, WorldError> {
        self.state_root_hash_for_state(&self.state)
    }

    pub(super) fn state_root_hash_with_industry_history_overlay(
        &self,
        prepared: &crate::state::industry_history_transition::PreparedIndustryHistoryEvent,
    ) -> Result<String, WorldError> {
        let projection =
            WorldStateProjection::borrowed(&self.state).with_industry_history_overlay(prepared);
        hash_json(&StateRootProjection {
            state: &projection,
            manifest_hash: &self.current_manifest_hash()?,
            policy_hash: &hash_json(&self.policies)?,
        })
    }

    pub(super) fn state_root_hash_with_governance_identity_profile_overlay(
        &self,
        target_agent_id: &str,
        next_profile: &GovernanceIdentityProfileState,
        allow_insert: bool,
    ) -> Result<String, WorldError> {
        let manifest_hash = self.current_manifest_hash()?;
        let policy_hash = hash_json(&self.policies)?;
        let state_projection = WorldStateProjection::borrowed(&self.state);
        let state_projection = if allow_insert {
            state_projection.with_governance_identity_profile_insert_overlay(
                target_agent_id,
                next_profile.clone(),
            )
        } else {
            state_projection
                .with_governance_identity_profile_overlay(target_agent_id, next_profile.clone())
        };
        hash_json(&StateRootProjection {
            state: &state_projection,
            manifest_hash: manifest_hash.as_str(),
            policy_hash: policy_hash.as_str(),
        })
    }

    pub(super) fn state_root_hash_with_governance_registry_overlay(
        &self,
        overlay: &super::governance_registry_publication::PreparedGovernanceRegistryEvent,
    ) -> Result<String, WorldError> {
        let manifest_hash = self.current_manifest_hash()?;
        let policy_hash = hash_json(&self.policies)?;
        let state =
            WorldStateProjection::borrowed(&self.state).with_governance_registry_overlay(overlay);
        hash_json(&StateRootProjection {
            state: &state,
            manifest_hash: manifest_hash.as_str(),
            policy_hash: policy_hash.as_str(),
        })
    }

    fn state_root_hash_for_state(&self, state: &WorldState) -> Result<String, WorldError> {
        let manifest_hash = self.current_manifest_hash()?;
        let policy_hash = hash_json(&self.policies)?;
        hash_json(&StateRootProjection {
            state,
            manifest_hash: manifest_hash.as_str(),
            policy_hash: policy_hash.as_str(),
        })
    }
}
