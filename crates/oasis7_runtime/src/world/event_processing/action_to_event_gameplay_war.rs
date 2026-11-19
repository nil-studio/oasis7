use super::*;
#[path = "../incremental_normal_action_war_evaluate.rs"]
pub(in crate::world) mod fixed;
impl World {
    pub(super) fn action_to_event_gameplay_war(
        &self,
        action_id: ActionId,
        action: &Action,
    ) -> Result<WorldEventBody, WorldError> {
        fixed::evaluate_war_action_v1(&mut fixed::LegacyWarFactsV1::new(self), action_id, action)
    }
}
