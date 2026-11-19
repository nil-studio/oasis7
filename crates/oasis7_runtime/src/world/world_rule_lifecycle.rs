//! The same business events drive native and builtin/module ticks.
use super::World;
use crate::{DomainEvent, WorldError, WorldEvent, WorldEventBody};
use oasis7_agent_api::world_action::WorldRuleCommandV1 as Command;
use super::incremental_normal_world_rule_lifecycle::work::{legacy_lifecycle_work_v1,LifecycleRequestV1,OrderLifecycleChoiceV1};

impl World {
    pub(crate) fn validate_world_rule_current_actor(
        &self,
        event: &DomainEvent,
    ) -> Result<(), WorldError> {
        let DomainEvent::WorldRuleCommitted {
            action_id,
            actor_id,
            request_id,
            command,
            ..
        } = event
        else {
            return Ok(());
        };
        if self
            .state
            .world_rules
            .as_ref()
            .is_some_and(|rules| rules.has_exact_request(actor_id, request_id, command))
        {
            return Ok(());
        }
        if *action_id != 0
            && self.state.is_manufactured_agent(actor_id)
            && self.current_agent_owner(actor_id).is_none()
        {
            return Err(crate::world_rules::invalid(
                "manufactured_actor_current_authority_unavailable",
            ));
        }
        if let Command::MinimumService {
            command:
                oasis7_agent_api::world_action::MinimumServiceCommandV1::Register {
                    registration, ..
                },
        } = command
            && self
                .state
                .is_manufactured_agent(&registration.beneficiary_id)
            && self
                .current_agent_owner(&registration.beneficiary_id)
                .is_none()
        {
            return Err(crate::world_rules::invalid(
                "minimum_service_beneficiary_current_authority_unavailable",
            ));
        }
        Ok(())
    }

    pub(super) fn process_world_rule_lifecycle(&mut self) -> Result<Vec<WorldEvent>, WorldError> {
        let now = self.state.time;
        let mut work = Vec::new();
        if let Some(rules) = &self.state.world_rules {
            for auth in rules.supply_authorizations.values() {
                if auth.active && !auth.is_valid(&self.state, now) {
                    work.push(legacy_lifecycle_work_v1(&auth.provider_id,LifecycleRequestV1::SupplyClose(&auth.id))?);
                }
            }
            for (id, shipment) in &rules.supply_shipments {
                if !shipment.settled && self.state.logistics_settlement_receipts.contains_key(id) {
                    let actor = rules.supply_authorizations[&shipment.authorization_id]
                        .provider_id
                        .clone();
                    work.push(legacy_lifecycle_work_v1(&actor,LifecycleRequestV1::SupplySettle(*id))?);
                }
            }
            for region in rules.emergency_supply.due_observation_regions(now) {
                if let Some(source) = rules
                    .emergency_supply
                    .minimum_services
                    .values()
                    .find(|s| s.registration.region_id == region)
                {
                    work.push(legacy_lifecycle_work_v1(&source.operator_agent_id,LifecycleRequestV1::Observe{region:&region,now})?);
                }
            }
            for order in rules
                .agent_orders
                .values()
                .filter(|o| !o.is_terminal() || o.refund_pending)
            {
                let choice = match order.phase {
                    crate::world_rules::AgentIndustrialPhaseV1::AwaitingDelivery
                        if self.industrial_order_blocker(order, now).is_none() => OrderLifecycleChoiceV1::Deliver,
                    _ => OrderLifecycleChoiceV1::Advance,
                };
                work.push(legacy_lifecycle_work_v1(&order.owner_id,LifecycleRequestV1::Order{order:&order.order_id,now,command:choice})?);
            }
            for auth in rules.emergency_supply.authorizations.values() {
                if auth.active
                    && auth.starts_at <= now
                    && !rules.emergency_supply.authorization_is_valid_for_world(
                        &self.state,
                        &auth.id,
                        now,
                    )
                {
                    work.push(legacy_lifecycle_work_v1(&auth.provider_id,LifecycleRequestV1::EmergencyClose(&auth.id))?);
                }
            }
        }
        let mut emitted = Vec::new();
        for (actor_id, request_id, command) in work {
            let event = DomainEvent::WorldRuleCommitted {
                action_id: 0,
                actor_id,
                request_id,
                command,
                governance_epoch: self.current_governance_epoch(),
                world_binding: self.world_rule_binding(),
            };
            // A newly changed prerequisite leaves the business pending; no
            // guessed completion or partial state is published.
            if self
                .validate_manufactured_identity_acquisition(&event)
                .is_err()
                || self.prepare_world_rule_event(&event, now).is_err()
            {
                continue;
            }
            self.append_event(WorldEventBody::Domain(event), None)?;
            if let Some(event) = self.journal.events.last() {
                emitted.push(event.clone());
            }
        }
        Ok(emitted)
    }
}
