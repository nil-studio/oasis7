use super::super::World;
use crate::state::core_policy_transition::PreparedCorePolicyEvent;
use crate::{WorldError, WorldStateProjection};
use oasis7_world_state::codec::{StateRootProjection, state_root_hash};

impl World {
    pub(super) fn state_root_hash_with_world_rule_overlay(
        &self,
        prepared: &crate::world_rules::PreparedWorldRule,
    ) -> Result<String, WorldError> {
        let projection =
            WorldStateProjection::borrowed(&self.state).with_world_rule_overlay(prepared);
        state_root_hash(&StateRootProjection {
            state: &projection,
            manifest_hash: &self.current_manifest_hash()?,
            policy_hash: &crate::util::hash_json(&self.policies)?,
        })
    }

    pub(super) fn state_root_hash_with_core_policy_overlay(
        &self,
        prepared: &PreparedCorePolicyEvent,
    ) -> Result<String, WorldError> {
        let projection =
            WorldStateProjection::borrowed(&self.state).with_core_policy_overlay(prepared);
        state_root_hash(&StateRootProjection {
            state: &projection,
            manifest_hash: &self.current_manifest_hash()?,
            policy_hash: &crate::util::hash_json(&self.policies)?,
        })
    }
}
