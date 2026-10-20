// One authorized projection for native API, Agent and Viewer consumers.
use crate::{World, WorldError};
use oasis7_agent_api::world_action::{SubstitutionPreviewRequestV1, SubstitutionPreviewV1};
use oasis7_world_state::world_rules::recipe_substitution;

impl World {
    /// The service must authenticate `actor` before granting this private read.
    pub fn recipe_substitution_preview(
        &self,
        actor: &str,
        request: &SubstitutionPreviewRequestV1,
    ) -> Result<SubstitutionPreviewV1, WorldError> {
        let world = self
            .world_rule_binding()
            .ok_or_else(|| super::invalid("substitution_world_binding_missing"))?;
        recipe_substitution::preview_substitution(
            self.state(),
            actor,
            request,
            &world,
            self.state().time,
        )
    }
}
#[cfg(test)]
mod tests {
    use crate::world_rules::{PreparedWorldRule, industrial_work_admission::tests as producer};

    #[test]
    fn new_owner_of_same_agent_cannot_read_old_root_lots_or_request_payloads() {
        let (mut state, key, command) = producer::root_fixture();
        let event = producer::event("root", command);
        PreparedWorldRule::prepare(&state, &event, 1)
            .unwrap()
            .install(&mut state);
        let event = producer::import(&state, &key, "import", producer::facts(&state));
        PreparedWorldRule::prepare(&state, &event, 1)
            .unwrap()
            .install(&mut state);
        let rules = state.world_rules.as_ref().unwrap();
        let current = rules.for_current_reader(&state, Some("owner"));
        assert_eq!(current.industrial_work.roots.len(), 1);
        assert_eq!(current.industrial_work.lots.len(), 1);
        assert_eq!(current.requests.len(), 2);
        assert!(
            rules
                .for_reader(Some("owner"))
                .industrial_work
                .roots
                .is_empty()
        );
        assert!(rules.for_reader(Some("owner")).requests.is_empty());
        assert!(
            rules
                .for_current_reader(&state, Some("other"))
                .industrial_work
                .roots
                .is_empty()
        );

        state.starter_oc_claims.get_mut("owner").unwrap().player_id = "replacement-player".into();
        let private = state
            .world_rules
            .as_ref()
            .unwrap()
            .for_current_reader(&state, Some("owner"));
        assert!(private.industrial_work.roots.is_empty());
        assert!(private.industrial_work.lots.is_empty());
        assert!(private.requests.is_empty());
        let status = state
            .recipe_substitution_status(Some("owner"), Some("world"))
            .unwrap();
        assert_eq!(status["roots"], serde_json::json!({}));
        assert_eq!(status["previews"], serde_json::json!([]));
        assert!(
            state
                .recipe_substitution_status(None, Some("world"))
                .is_none()
        );
    }
}
