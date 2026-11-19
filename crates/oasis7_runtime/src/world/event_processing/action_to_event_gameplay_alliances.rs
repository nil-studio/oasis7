use super::*;
#[path = "../incremental_normal_action_diplomacy_evaluate.rs"]
pub(in crate::world) mod fixed;
impl World {
    pub(super) fn action_to_event_gameplay_alliances(
        &self,
        action_id: ActionId,
        action: &Action,
    ) -> Result<WorldEventBody, WorldError> {
        fixed::evaluate_alliance_action_v1(
            &mut fixed::LegacyAllianceFactsV1::new(self),
            action_id,
            action,
        )
    }
}
