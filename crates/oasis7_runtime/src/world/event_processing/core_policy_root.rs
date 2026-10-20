use super::super::World;
use crate::state::core_policy_transition::PreparedCorePolicyEvent;
use crate::{WorldError, WorldStateProjection};
use serde::Serialize;

#[derive(Serialize)]
struct StateRootProjection<'a> {
    state: &'a WorldStateProjection<'a>,
    manifest_hash: &'a str,
    policy_hash: &'a str,
}

impl World {
    pub(super) fn state_root_hash_with_world_rule_overlay(
        &self,
        prepared: &crate::world_rules::PreparedWorldRule,
    ) -> Result<String, WorldError> {
        let projection =
            WorldStateProjection::borrowed(&self.state).with_world_rule_overlay(prepared);
        crate::util::hash_json(&StateRootProjection {
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
        crate::util::hash_json(&StateRootProjection {
            state: &projection,
            manifest_hash: &self.current_manifest_hash()?,
            policy_hash: &crate::util::hash_json(&self.policies)?,
        })
    }
}
