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

    /// Queue a controller-signed governance command. Authorization is evaluated
    /// against the qualified parent during execution, not at enqueue time.
    pub fn submit_governance_action(
        &mut self,
        action: Action,
        authorization: oasis7_agent_api::world_action::GovernanceAuthorizationV1,
    ) -> ActionId {
        let id = self.allocate_next_action_id();
        self.pending_actions
            .push_back(submission_envelope(id, action, Some(authorization), None));
        self.enforce_pending_action_limit();
        id
    }

    /// Queue committed correlation metadata without changing action authorization.
    pub fn submit_recipe_action_with_origin(
        &mut self,
        action: Action,
        origin: super::super::CommittedRecipeOrigin,
    ) -> Result<ActionId, super::super::WorldError> {
        validate_committed_recipe_submission(&action, &origin)?;
        let action_id = self.allocate_next_action_id();
        self.pending_actions
            .push_back(submission_envelope(action_id, action, None, Some(origin)));
        self.enforce_pending_action_limit();
        Ok(action_id)
    }

    /// Queue a World action under a previously allocated durable ID. Cognition
    /// commit finalization uses this to keep the receipt's action_id identical
    /// to the ActionEnvelope that the step actually executes.
    pub(super) fn submit_action_with_id(&mut self, action_id: ActionId, action: Action) {
        self.pending_actions
            .push_back(submission_envelope(action_id, action, None, None));
        self.enforce_pending_action_limit();
    }

    pub fn pending_actions_len(&self) -> usize {
        self.pending_actions.len()
    }

    pub fn pending_effects_len(&self) -> usize {
        self.pending_effects.len()
    }
}

/// Shared pre-allocation recipe admission. Both submission drivers must call
/// this before consuming the durable action allocator.
pub(super) fn validate_committed_recipe_submission(
    action: &Action,
    origin: &super::super::CommittedRecipeOrigin,
) -> Result<(), super::super::WorldError> {
    match action {
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
    Ok(())
}

/// Exact envelope materialization shared by legacy and incremental submission.
/// Recipe correlation validation precedes allocator consumption at both callers;
/// execution-time governance authorization is deliberately preserved verbatim.
pub(super) fn submission_envelope(
    id: ActionId,
    action: Action,
    governance_authorization: Option<oasis7_agent_api::world_action::GovernanceAuthorizationV1>,
    committed_recipe_origin: Option<super::super::CommittedRecipeOrigin>,
) -> ActionEnvelope {
    ActionEnvelope {
        id,
        action,
        governance_authorization,
        committed_recipe_origin,
    }
}
