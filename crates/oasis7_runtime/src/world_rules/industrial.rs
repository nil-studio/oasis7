use super::*;
use crate::*;
use oasis7_agent_api::world_action::WorldRuleCommandV1;
use std::collections::BTreeMap;

impl crate::World {
    pub(crate) fn industrial_owner_identity_conflicts(&self, owner: &str, player: &str) -> bool {
        self.current_agent_identity(owner)
            .is_some_and(|identity| identity.owner_binding != player)
    }

    pub(crate) fn industrial_order_blocker(
        &self,
        order: &AgentIndustrialOrderV1,
        now: WorldTime,
    ) -> Option<String> {
        if let Err(error) = order.validate() {
            return Some(format!("{error:?}"));
        }
        if order.quote.world_binding != self.world_rule_binding() {
            return Some("manufacturing_world_binding_mismatch".into());
        }
        if self.industrial_owner_identity_conflicts(&order.owner_id, &order.quote.owner_player_id) {
            return Some("manufacturing_owner_identity_changed".into());
        }
        if order.phase == AgentIndustrialPhaseV1::Producing {
            if order
                .production_deadline()
                .is_some_and(|deadline| now >= deadline)
            {
                return Some("manufacturing_production_expired".into());
            }
            return order_production_eligibility(self.state(), order)
                .err()
                .map(|error| format!("{error:?}"));
        }
        order.blocker(self.state(), now)
    }

    /// Add only the World-owned identity conflict to the existing economic
    /// eligibility view. The temporary policy mark is never part of the
    /// prepared delta: expired orders still release capacity while their
    /// refunds stay in escrow until the original identity is restored.
    pub(crate) fn prepare_world_rule_event(
        &self,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<PreparedWorldRule, WorldError> {
        let DomainEvent::WorldRuleCommitted {
            actor_id,
            request_id,
            command,
            ..
        } = event
        else {
            return PreparedWorldRule::prepare(self.state(), event, now);
        };
        if self
            .state()
            .world_rules
            .as_ref()
            .is_some_and(|rules| rules.has_exact_request(actor_id, request_id, command))
        {
            return PreparedWorldRule::prepare(self.state(), event, now);
        }
        let principal = match command {
            WorldRuleCommandV1::ConfirmAgentOrder { .. } => self
                .state()
                .resolved_agent_owner(actor_id)
                .map(|owner| (actor_id.clone(), owner.player_id)),
            WorldRuleCommandV1::AdvanceAgentOrder { order_id }
            | WorldRuleCommandV1::DeliverAgentOrder { order_id }
            | WorldRuleCommandV1::CancelAgentOrder { order_id }
            | WorldRuleCommandV1::DisposeAgentOrder { order_id } => self
                .state()
                .world_rules
                .as_ref()
                .and_then(|rules| rules.agent_orders.get(order_id))
                .map(|order| (order.owner_id.clone(), order.quote.owner_player_id.clone())),
            _ => None,
        };
        let Some((owner, _player)) = principal
            .filter(|(owner, player)| self.industrial_owner_identity_conflicts(owner, player))
        else {
            return PreparedWorldRule::prepare(self.state(), event, now);
        };
        if matches!(
            command,
            WorldRuleCommandV1::CancelAgentOrder { .. }
                | WorldRuleCommandV1::DisposeAgentOrder { .. }
        ) {
            return Err(invalid("manufacturing_owner_identity_changed"));
        }
        let mut authority_view = self.state().clone();
        if !authority_view
            .gameplay_policy
            .blocked_agents
            .contains(&owner)
        {
            authority_view.gameplay_policy.blocked_agents.push(owner);
        }
        PreparedWorldRule::prepare(&authority_view, event, now)
    }
}
impl crate::World {
    pub(crate) fn industrial_acquisition_rejection(
        &self,
        claimer: &str,
        target: &str,
    ) -> Option<crate::RejectReason> {
        if self.state().is_manufactured_agent(claimer) {
            return Some(crate::RejectReason::RuleDenied {
                notes: vec!["manufactured_actor_cannot_acquire_agent".into()],
            });
        }
        let bound = self.state().starter_oc_claims.contains_key(claimer);
        let note = if (bound || self.claimed_agent_count(claimer) > 0)
            && !self.state().is_manufactured_agent(target)
        {
            Some("additional_agent_requires_industrial_delivery".to_owned())
        } else if bound {
            manufacturing_capacity_guard(self.state(), claimer, Some(target), false)
                .err()
                .map(|error| format!("{error:?}"))
        } else {
            None
        };
        note.map(|note| crate::RejectReason::RuleDenied { notes: vec![note] })
    }

    pub(crate) fn add_manufacturing_factory_workload(
        &self,
        active_jobs_by_factory: &mut BTreeMap<String, i64>,
        now: WorldTime,
    ) {
        if let Some(rules) = &self.state().world_rules {
            for order in rules.agent_orders.values().filter(|order| {
                order.phase == crate::world_rules::AgentIndustrialPhaseV1::Producing
                    && self.industrial_order_blocker(order, now).is_none()
            }) {
                active_jobs_by_factory
                    .entry(order.factory_id.clone())
                    .and_modify(|count| *count = count.saturating_add(1))
                    .or_insert(1);
            }
        }
    }
}
