//! Conditional native construction preview, using admission on a private clone.
use super::*;
use crate::{
    Action, AgentDecisionCausalContextV1, DomainEvent, WorldEventBody, builtin_factory_spec,
};
use oasis7_agent_api::world_action::ResourceKind;

pub use oasis7_agent_api::world_quote::{BuiltinFactoryStartPreview, NativeBuiltinFactoryQuote};

impl World {
    pub fn builtin_factory_quote(
        &self,
        agent: &str,
        site: &str,
        kind: &str,
        factory: &str,
    ) -> Result<NativeBuiltinFactoryQuote, WorldError> {
        let invalid = |reason: &str| WorldError::DistributedValidationFailed {
            reason: reason.into(),
        };
        let spec = builtin_factory_spec(kind, factory)
            .ok_or_else(|| invalid("unsupported builtin factory selection"))?;
        let action = Action::BuildFactory {
            builder_agent_id: agent.into(),
            site_id: site.into(),
            spec: spec.clone(),
        };
        let mut grants = self
            .agent_delegation_authorizations(agent)?
            .into_iter()
            .filter(|a| {
                a.grant.object_id == factory
                    && a.grant.action_kinds.iter().any(|v| v == "BuildFactory")
            });
        let mut authorization = grants
            .next()
            .ok_or_else(|| invalid("native factory grant required"))?;
        if grants.next().is_some() {
            return Err(invalid("native factory grant ambiguous"));
        }
        let mut clone = self.clone();
        let request = "builtin-factory-read-only-preview";
        clone.bind_agent_causal_decision(
            request,
            agent,
            &action,
            AgentDecisionCausalContextV1::default(),
        )?;
        clone.prepare_agent_delegation_commit(request, &action)?;
        let body = clone.action_to_event(&ActionEnvelope {
            id: 0,
            action: action.clone(),
            committed_recipe_origin: None,
        })?;
        let power = self
            .state
            .factory_construction_power_profiles
            .get(factory)
            .ok_or_else(|| invalid("construction power unavailable"))?;
        authorization.cost_units = u64::try_from(power.electricity_amount)
            .map_err(|_| invalid("invalid construction power"))?;
        let electricity_before = self
            .state
            .agents
            .get(agent)
            .ok_or_else(|| invalid("factory agent unavailable"))?
            .state
            .resources
            .get(ResourceKind::Electricity);
        let (rejection, preview, electricity_after) = match body {
            WorldEventBody::Domain(DomainEvent::FactoryBuildStarted {
                consume_ledger,
                ready_at,
                site_authority_revision: Some(site_authority_revision),
                site_location_id: Some(site_location_id),
                location_anchor_revision: Some(location_anchor_revision),
                construction_power_obligation: Some(construction_power_obligation),
                ..
            }) => {
                let duration_ticks = u32::try_from(
                    ready_at
                        .checked_sub(self.state.time)
                        .ok_or_else(|| invalid("invalid factory ready tick"))?,
                )
                .map_err(|_| invalid("factory duration overflow"))?;
                let after = construction_power_obligation.electricity_after;
                (
                    None,
                    Some(BuiltinFactoryStartPreview {
                        consume_ledger,
                        consume: spec.build_cost.clone(),
                        duration_ticks,
                        site_authority_revision,
                        site_location_id,
                        location_anchor_revision,
                        construction_power_obligation,
                    }),
                    after,
                )
            }
            WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => {
                (Some(reason), None, None)
            }
            _ => return Err(invalid("native factory preview unavailable")),
        };
        let output = NativeBuiltinFactoryQuote {
            action,
            quoted_at_tick: self.state.time,
            authorization,
            electricity_before,
            electricity_after,
            rejection,
            preview,
        };
        output
            .validate_selection(agent, site, kind, factory)
            .map_err(|_| invalid("native factory quote inconsistent"))?;
        Ok(output)
    }
}
