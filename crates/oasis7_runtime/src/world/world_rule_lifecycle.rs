//! The same business events drive native and builtin/module ticks.
use super::World;
use crate::world_rules::{
    OrganizationDispositionPhaseV1 as Disposition, OrganizationProtectionPhaseV1 as Phase,
};
use crate::{DomainEvent, WorldError, WorldEvent, WorldEventBody};
use oasis7_agent_api::world_action::WorldRuleCommandV1 as Command;

impl World {
    pub(super) fn process_world_rule_lifecycle(&mut self) -> Result<Vec<WorldEvent>, WorldError> {
        let now = self.state.time;
        let mut work = Vec::new();
        if let Some(rules) = &self.state.world_rules {
            for auth in rules.supply_authorizations.values() {
                if auth.active && !auth.is_valid(&self.state, now) {
                    work.push((
                        auth.provider_id.clone(),
                        format!("close:{}", crate::util::hash_json(&auth.id)?),
                        Command::CloseSupply {
                            authorization_id: auth.id.clone(),
                        },
                    ));
                }
            }
            for (id, shipment) in &rules.supply_shipments {
                if !shipment.settled && self.state.logistics_settlement_receipts.contains_key(id) {
                    let actor = rules.supply_authorizations[&shipment.authorization_id]
                        .provider_id
                        .clone();
                    work.push((
                        actor,
                        format!("settle:{id}"),
                        Command::SettleSupply { job_id: *id },
                    ));
                }
            }
            for order in rules.agent_orders.values() {
                if self
                    .state
                    .agent_industrial_order_blocker(order, now)
                    .is_none()
                {
                    work.push((
                        order.owner_id.clone(),
                        format!("deliver:{}", crate::util::hash_json(&order.order_id)?),
                        Command::DeliverAgentOrder {
                            order_id: order.order_id.clone(),
                        },
                    ));
                }
            }
            for case in rules.organization_cases.values() {
                let due = case.phase == Phase::Notice
                    && case
                        .notified_at
                        .is_some_and(|t| now >= t.saturating_add(120))
                    || case.phase == Phase::Limited
                        && case
                            .grants
                            .values()
                            .all(|g| now >= g.expires_at || g.scopes.is_empty());
                if due {
                    work.push((
                        case.founder_id.clone(),
                        format!(
                            "advance:{}:{}",
                            crate::util::hash_json(&case.id)?,
                            case.revision
                        ),
                        Command::AdvanceOrganization {
                            case_id: case.id.clone(),
                            expected_revision: case.revision,
                        },
                    ));
                }
            }
        }
        for alliance in self.state.alliances.values() {
            let has_case = self.state.world_rules.as_ref().is_some_and(|r| {
                r.organization_cases.values().any(|c| {
                    c.organization_id == alliance.alliance_id
                        && c.phase != Phase::Resolved
                        && c.disposition != Disposition::Complete
                })
            });
            let activity = alliance
                .members
                .iter()
                .map(|id| self.state.agents.get(id).map(|a| a.last_active))
                .collect::<Option<Vec<_>>>();
            if !has_case
                && !alliance.members.is_empty()
                && activity.is_some_and(|v| v.into_iter().all(|at| now.saturating_sub(at) >= 600))
            {
                work.push((
                    alliance.formed_by_agent_id.clone(),
                    format!(
                        "notice:{}:{now}",
                        crate::util::hash_json(&alliance.alliance_id)?
                    ),
                    Command::OpenOrganizationProtection {
                        organization_id: alliance.alliance_id.clone(),
                    },
                ));
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
            };
            // A newly changed prerequisite leaves the business pending; no
            // guessed completion or partial state is published.
            if crate::world_rules::PreparedWorldRule::prepare(&self.state, &event, now).is_err() {
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
