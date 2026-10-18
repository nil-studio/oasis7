//! Conditional native production preview; never applies events or reserves assets.
use super::*;
use crate::{
    Action, AgentDecisionCausalContextV1, DomainEvent, WorldEventBody, builtin_recipe_plan,
};
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
        let mut authorizations = self
            .agent_delegation_authorizations(agent)?
            .into_iter()
            .filter(|a| {
                a.grant.object_id == factory
                    && a.grant.action_kinds.iter().any(|k| k == "ScheduleRecipe")
            });
        let mut authorization = authorizations
            .next()
            .ok_or_else(|| invalid("native recipe grant required"))?;
        if authorizations.next().is_some() {
            return Err(invalid("native recipe grant ambiguous"));
        }
        // Binding happens only on the private clone. Reuse real grant and domain
        // admission without stepping the world or completing unrelated jobs.
        let mut preview_world = self.clone();
        let request_id = "builtin-recipe-read-only-preview";
        preview_world.bind_agent_causal_decision(
            request_id,
            agent,
            &action,
            AgentDecisionCausalContextV1::default(),
        )?;
        preview_world.prepare_agent_delegation_commit(request_id, &action)?;
        let body = preview_world.action_to_event(&ActionEnvelope {
            id: 0,
            action: action.clone(),
            committed_recipe_origin: None,
        })?;
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
        let (rejection, preview, electricity_after) = match body {
            WorldEventBody::Domain(DomainEvent::RecipeStarted {
                consume_ledger,
                output_ledger,
                consume,
                produce,
                byproducts,
                power_required,
                duration_ticks,
                ..
            }) => (
                None,
                Some(BuiltinRecipeStartPreview {
                    consume_ledger,
                    output_ledger,
                    consume,
                    produce,
                    byproducts,
                    power_required,
                    duration_ticks,
                }),
                electricity_before.checked_sub(power_required),
            ),
            WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => {
                (Some(reason), None, None)
            }
            _ => return Err(invalid("native recipe preview unavailable")),
        };
        let output = NativeBuiltinRecipeQuote {
            action,
            quoted_at_tick: self.state.time,
            authorization,
            electricity_before,
            electricity_after,
            rejection,
            preview,
        };
        output
            .validate_selection(agent, factory, recipe, batches)
            .map_err(|_| invalid("native recipe quote inconsistent"))?;
        Ok(output)
    }
}
