//! Conditional native production preview; never applies events or reserves assets.
use super::*;
use crate::{Action, DomainEvent, WorldEventBody, builtin_recipe_plan};
use oasis7_agent_api::world_action::ResourceKind;

pub use oasis7_agent_api::world_quote::{BuiltinRecipeStartPreview, NativeBuiltinRecipeQuote};

impl World {
    pub fn builtin_recipe_quote(
        &self,
        agent: &str,
        factory: &str,
        recipe: &str,
        batches: u32,
    ) -> Result<NativeBuiltinRecipeQuote, WorldError> {
        let invalid = |reason: &str| WorldError::DistributedValidationFailed {
            reason: reason.into(),
        };
        let plan = builtin_recipe_plan(recipe, batches)
            .ok_or_else(|| invalid("unsupported builtin recipe selection"))?;
        let action = Action::ScheduleRecipe {
            requester_agent_id: agent.into(),
            factory_id: factory.into(),
            recipe_id: recipe.into(),
            plan: plan.clone(),
            logistics_route_ids: vec![],
            logistics_path_ids: vec![],
        };
        let binding = super::agent_delegation::readonly::prepare_recipe_preview_binding_v1(
            self, agent, &action,
        )?;
        let mut authorization = binding.authorization().clone();
        let body = self.action_to_event_with_native_recipe_preview_v1(
            &ActionEnvelope {
                id: 0,
                action: action.clone(),
                governance_authorization: None,
                committed_recipe_origin: None,
            },
            Some(&binding),
        )?;
        authorization.cost_units =
            u64::try_from(plan.power_required).map_err(|_| invalid("invalid canonical power"))?;
        let electricity_before = self
            .state
            .agents
            .get(agent)
            .ok_or_else(|| invalid("recipe agent unavailable"))?
            .state
            .resources
            .get(ResourceKind::Electricity);
        readonly::from_event_v1(
            action,
            authorization,
            body,
            electricity_before,
            self.state.time,
            agent,
            factory,
            recipe,
            batches,
            &mut super::agent_delegation::readonly::LegacyWork,
        )
    }
}
#[path = "builtin_recipe_quote_readonly.rs"]
pub(in crate::world) mod readonly;
