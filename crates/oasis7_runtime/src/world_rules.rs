pub use oasis7_world_state::world_rules::*;
pub(crate) mod action_results;
pub mod industrial;
pub mod substitution_read;

impl crate::World {
    pub fn world_rule_status(&self, reader: Option<&str>) -> Option<serde_json::Value> {
        match status_readonly::status_from_sources_v1(&status_readonly::Legacy(self),reader){Ok(value)=>value,Err(never)=>match never{}}
    }
    pub fn can_read_agent_industrial_order(
        &self,
        order: &AgentIndustrialOrderV1,
        reader: Option<&str>,
    ) -> bool {
        match status_readonly::can_read_order_v1(&status_readonly::Legacy(self),order,reader){Ok(value)=>value,Err(never)=>match never{}}
    }
}
#[cfg(test)]
use crate::*;
#[cfg(test)]
use oasis7_agent_api::world_action::WorldRuleCommandV1;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(crate) fn emergency_authorize_fixture() -> (crate::World, oasis7_agent_api::world_action::Action)
{
    tests::emergency_authorize_fixture()
}
#[cfg(test)]
#[cfg(all(feature = "wasmtime", feature = "test_tier_full"))]
pub(crate) fn essential_recipe_fixture() -> (crate::World, oasis7_agent_api::world_action::Action) {
    tests::essential_recipe_fixture()
}
#[cfg(test)]
pub mod industrial_work_admission;
#[cfg(test)]
pub mod recipe_substitution;

#[cfg(test)]
use std::collections::BTreeMap;

/// Same read-only capability facts for authenticated Service and Viewer adapters.
/// `execution_actor` is supplied only after the entry point verifies its current
/// owner authority. Merely observing the personal subject never enables actions.
impl crate::World {
    pub fn gameplay_capability_query(
        &self,
        subject: oasis7_world_state::gameplay_capability::GameplaySubject,
        goal_id: &str,
        execution_actor: Option<&str>,
    ) -> oasis7_world_state::gameplay_capability::GameplayCapabilityQuery {
        let mut query=self.state().gameplay_capability_query(subject,goal_id);
        match readonly_recovery::append_recipe_recovery_data_v1(&readonly_recovery::LegacyRecoveryV1(self),&mut query,goal_id,execution_actor){Ok(())=>query,Err(never)=>match never{}}

    }
}

#[path="world/incremental_normal_projection_commitment_recovery_rules.rs"]
pub(crate) mod readonly_recovery;

pub(crate) mod status_readonly;
