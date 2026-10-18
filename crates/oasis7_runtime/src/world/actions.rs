use super::super::{Action, ActionEnvelope, ActionId};
use super::World;

impl World {
    // ---------------------------------------------------------------------
    // Action submission
    // ---------------------------------------------------------------------

    pub fn submit_action(&mut self, action: Action) -> ActionId {
        let action_id = self.allocate_next_action_id();
        self.submit_action_with_id(action_id, action);
        action_id
    }

    /// Queue committed correlation metadata without changing action authorization.
    pub fn submit_recipe_action_with_origin(
        &mut self,
        action: Action,
        origin: super::super::CommittedRecipeOrigin,
    ) -> Result<ActionId, super::super::WorldError> {
        match &action {
            Action::ScheduleRecipe {
                requester_agent_id,
                factory_id,
                recipe_id,
                ..
            } if origin.matches_recipe(requester_agent_id, factory_id, recipe_id) => {}
            _ => {
                return Err(super::super::WorldError::ResourceBalanceInvalid {
                    reason: "committed recipe origin action mismatch".to_string(),
                });
            }
        }
        let action_id = self.allocate_next_action_id();
        self.pending_actions.push_back(ActionEnvelope {
            id: action_id,
            action,
            committed_recipe_origin: Some(origin),
        });
        self.enforce_pending_action_limit();
        Ok(action_id)
    }

    /// Queue a World action under a previously allocated durable ID. Cognition
    /// commit finalization uses this to keep the receipt's action_id identical
    /// to the ActionEnvelope that the step actually executes.
    pub(super) fn submit_action_with_id(&mut self, action_id: ActionId, action: Action) {
        self.pending_actions.push_back(ActionEnvelope {
            committed_recipe_origin: None,
            id: action_id,
            action,
        });
        self.enforce_pending_action_limit();
    }

    pub fn pending_actions_len(&self) -> usize {
        self.pending_actions.len()
    }

    pub fn pending_effects_len(&self) -> usize {
        self.pending_effects.len()
    }
}
