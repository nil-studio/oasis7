//! Authoritative v1 world-rule business state and sparse transaction preparation.
use crate::{AgentCell, DomainEvent, MaterialLedgerId, WorldError, WorldState, WorldTime};
use oasis7_agent_api::world_action::WorldRuleCommandV1;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod industrial;
mod organization;
mod supply;
pub use organization::*;
pub use supply::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WorldRulesStateV1 {
    pub requests: BTreeMap<String, WorldRuleReceiptV1>,
    pub governance_effects: BTreeMap<String, String>,
    pub organization_cases: BTreeMap<String, OrganizationCaseV1>,
    pub organization_estate: BTreeMap<String, OrganizationEstateDepositV1>,
    pub supply_authorizations: BTreeMap<String, SupplyAuthorizationV1>,
    pub supply_batches: BTreeMap<String, SupplyBatchV1>,
    pub supply_lineages: BTreeMap<String, SupplyLineageV1>,
    pub supply_shipments: BTreeMap<u64, SupplyShipmentV1>,
    pub agent_orders: BTreeMap<String, AgentIndustrialOrderV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldRuleReceiptV1 {
    pub request_id: String,
    pub actor_id: String,
    pub command: WorldRuleCommandV1,
    pub committed_at: WorldTime,
    pub result_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentIndustrialOrderV1 {
    pub order_id: String,
    pub owner_id: String,
    pub factory_id: String,
    pub target_agent_id: String,
    pub policy_version: u32,
    pub inputs: BTreeMap<String, i64>,
    pub confirmed_at: WorldTime,
    pub ready_at: WorldTime,
    pub delivered_at: Option<WorldTime>,
}
#[derive(Debug)]
pub(crate) struct PreparedWorldRule {
    pub(crate) event: DomainEvent,
    pub(crate) rules: WorldRulesStateV1,
    pub(crate) ledgers: BTreeMap<MaterialLedgerId, BTreeMap<String, i64>>,
    pub(crate) agents: BTreeMap<String, AgentCell>,
    pub(crate) removed_alliance: Option<String>,
    pub(crate) industry: Option<crate::state::industry_transition::PreparedIndustryEvent>,
    pub(crate) claim:
        Option<crate::world::agent_claim_economic_publication::PreparedAgentClaimEconomic>,
}
impl PreparedWorldRule {
    pub(crate) fn prepare(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        let DomainEvent::WorldRuleCommitted {
            actor_id,
            request_id,
            command,
            governance_epoch,
            ..
        } = event
        else {
            return Err(invalid("unsupported rule event"));
        };
        validate_id(actor_id)?;
        validate_id(request_id)?;
        if !state.agents.contains_key(actor_id) {
            return Err(invalid("actor_not_found"));
        }
        let key = request_key(actor_id, request_id);
        let mut next = Self {
            event: event.clone(),
            rules: state.world_rules.clone().unwrap_or_default(),
            ledgers: BTreeMap::new(),
            agents: BTreeMap::from([(actor_id.clone(), state.agents[actor_id].clone())]),
            removed_alliance: None,
            industry: None,
            claim: None,
        };
        if let Some(receipt) = next.rules.requests.get(&key) {
            if receipt.actor_id != *actor_id || receipt.command != *command {
                return Err(invalid("request_content_conflict"));
            }
            return Ok(next);
        }
        if command
            .governance_proposal()
            .is_some_and(|id| next.rules.governance_effects.contains_key(id))
        {
            return Err(invalid("governance_proposal_already_consumed"));
        }
        if state
            .gameplay_policy
            .blocked_agents
            .iter()
            .any(|id| id == actor_id)
            && !matches!(
                command,
                WorldRuleCommandV1::CloseSupply { .. } | WorldRuleCommandV1::SettleSupply { .. }
            )
            && !matches!(
                event,
                DomainEvent::WorldRuleCommitted {
                    action_id: 0,
                    command: WorldRuleCommandV1::OpenOrganizationProtection { .. },
                    ..
                }
            )
            && !matches!(event,DomainEvent::WorldRuleCommitted{action_id:0,command:WorldRuleCommandV1::AdvanceOrganization{case_id,..},..} if next.rules.organization_cases.get(case_id).is_some_and(|c|c.phase==OrganizationProtectionPhaseV1::Notice || c.phase==OrganizationProtectionPhaseV1::Limited && c.grants.values().all(|g|g.expires_at<=now||g.scopes.is_empty())))
        {
            return Err(invalid("actor_blocked"));
        }
        if matches!(event,DomainEvent::WorldRuleCommitted {action_id,..} if *action_id>0) {
            next.agents
                .get_mut(actor_id)
                .expect("validated actor")
                .last_active = now;
        }
        let result_id = match command {
            WorldRuleCommandV1::ConfirmAgentOrder { .. }
            | WorldRuleCommandV1::DeliverAgentOrder { .. } => {
                next.industrial(state, actor_id, &key, command, *governance_epoch, now)?
            }
            WorldRuleCommandV1::AuthorizeSupply { .. }
            | WorldRuleCommandV1::RequestSupply { .. }
            | WorldRuleCommandV1::FreezeSupply { .. }
            | WorldRuleCommandV1::AllocateSupply { .. }
            | WorldRuleCommandV1::DispatchSupply { .. }
            | WorldRuleCommandV1::SettleSupply { .. }
            | WorldRuleCommandV1::CloseSupply { .. } => {
                next.supply(state, actor_id, &key, command, now)?
            }
            _ => next.organization(state, actor_id, &key, command, now)?,
        };
        if let Some(id) = command.governance_proposal() {
            next.rules
                .governance_effects
                .insert(id.into(), result_id.clone());
        }
        next.rules.requests.insert(
            key,
            WorldRuleReceiptV1 {
                request_id: request_id.clone(),
                actor_id: actor_id.clone(),
                command: command.clone(),
                committed_at: now,
                result_id,
            },
        );
        Ok(next)
    }
    pub(crate) fn install(self, state: &mut WorldState) {
        if let Some(claim) = self.claim {
            claim.install_infallible(state);
        } else if let Some(industry) = self.industry {
            industry.install(state);
        } else {
            state.agents.extend(self.agents);
        }
        state.material_ledgers.extend(self.ledgers);
        if let Some(id) = self.removed_alliance {
            state.alliances.remove(&id);
        }
        state.world_rules = Some(self.rules);
    }
    fn debit(
        &mut self,
        state: &WorldState,
        ledger: &MaterialLedgerId,
        inputs: &BTreeMap<String, i64>,
    ) -> Result<(), WorldError> {
        let values = self.ledgers.entry(ledger.clone()).or_insert_with(|| {
            state
                .material_ledgers
                .get(ledger)
                .cloned()
                .unwrap_or_default()
        });
        for (kind, amount) in inputs {
            if *amount <= 0 || values.get(kind).copied().unwrap_or(0) < *amount {
                return Err(invalid(format!("insufficient_material:{kind}")));
            }
        }
        for (kind, amount) in inputs {
            *values.entry(kind.clone()).or_default() -= amount;
        }
        Ok(())
    }
}
pub(crate) fn invalid(reason: impl Into<String>) -> WorldError {
    WorldError::ResourceBalanceInvalid {
        reason: reason.into(),
    }
}
fn validate_id(id: &str) -> Result<(), WorldError> {
    if id.is_empty() || id.len() > 128 || id.trim() != id {
        Err(invalid("invalid_identity"))
    } else {
        Ok(())
    }
}
fn request_key(actor: &str, request: &str) -> String {
    let mut hash = blake3::Hasher::new();
    hash.update(b"oasis7.world-rule.request.v1");
    hash.update(&(actor.len() as u64).to_le_bytes());
    hash.update(actor.as_bytes());
    hash.update(&(request.len() as u64).to_le_bytes());
    hash.update(request.as_bytes());
    format!("rule:{}", hash.finalize().to_hex())
}

impl WorldState {
    /// Read-only order availability; no identity or claim is created by a quote.
    pub fn agent_industrial_order_blocker(
        &self,
        order: &AgentIndustrialOrderV1,
        now: WorldTime,
    ) -> Option<String> {
        if order.delivered_at.is_some() {
            return Some("already_delivered".into());
        }
        if now < order.ready_at {
            return Some(format!("wait_until:{}", order.ready_at));
        }
        let Some(factory) = self.factories.get(&order.factory_id) else {
            return Some("factory_missing".into());
        };
        if factory.builder_agent_id != order.owner_id || factory.durability_ppm <= 0 {
            return Some("repair_or_restore_factory".into());
        }
        if let Err(error) = industrial::funding_for_order(self, &order.owner_id) {
            return Some(format!("fund_or_restore_claim_eligibility:{error:?}"));
        }
        None
    }
}

impl PreparedWorldRule {
    pub(crate) fn serialize_agents<S: serde::ser::SerializeStruct>(
        &self,
        state: &WorldState,
        output: &mut S,
    ) -> Result<(), S::Error> {
        if let Some(claim) = &self.claim {
            return claim.serialize_agents(state, output);
        }
        if let Some(industry) = &self.industry {
            return industry.serialize_agents(state, output);
        }
        let mut agents = self.agents.clone();
        if let Some(id) = self.event.agent_id() {
            if let Some(agent) = agents.get_mut(id) {
                agent.mailbox.push_back(self.event.clone());
            }
        }
        output.serialize_field(
            "agents",
            &crate::world::economy_data_publication::SparseMapProjection {
                base: &state.agents,
                updates: &agents,
                deletion: None,
            },
        )
    }
    pub(crate) fn serialize_materials<S: serde::ser::SerializeStruct>(
        &self,
        state: &WorldState,
        output: &mut S,
    ) -> Result<(), S::Error> {
        if let Some(claim) = &self.claim {
            return claim.serialize_materials(state, output);
        }
        if let Some(industry) = &self.industry {
            return industry.serialize_materials(state, output);
        }
        output.serialize_field("materials", &state.materials)?;
        output.serialize_field(
            "material_ledgers",
            &crate::world::economy_data_publication::SparseMapProjection {
                base: &state.material_ledgers,
                updates: &self.ledgers,
                deletion: None,
            },
        )
    }
}

impl PreparedWorldRule {
    fn credit(
        &mut self,
        state: &WorldState,
        ledger: &MaterialLedgerId,
        kind: &str,
        amount: i64,
    ) -> Result<(), WorldError> {
        if amount < 0 {
            return Err(invalid("negative_credit"));
        }
        let values = self.ledgers.entry(ledger.clone()).or_insert_with(|| {
            state
                .material_ledgers
                .get(ledger)
                .cloned()
                .unwrap_or_default()
        });
        let balance = values.entry(kind.into()).or_default();
        *balance = balance
            .checked_add(amount)
            .ok_or_else(|| invalid("material_overflow"))?;
        Ok(())
    }
}
fn require_governance(
    state: &WorldState,
    actor: &str,
    proposal_key: &str,
    command: &WorldRuleCommandV1,
) -> Result<(), WorldError> {
    let p = state
        .governance_proposals
        .get(proposal_key)
        .ok_or_else(|| invalid("governance_proposal_missing"))?;
    let binding = oasis7_agent_api::world_action::world_rule_proposal_description(command)
        .map_err(|_| invalid("invalid_governance_binding"))?;
    if p.proposer_agent_id != actor
        || p.status != crate::GovernanceProposalStatus::Passed
        || p.total_weight_at_finalize < 3
        || p.description != binding
    {
        return Err(invalid("bound_governance_approval_required"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;

impl PreparedWorldRule {
    pub(crate) fn serialize_alliances<S: serde::ser::SerializeStruct>(
        &self,
        state: &WorldState,
        output: &mut S,
    ) -> Result<(), S::Error> {
        output.serialize_field(
            "alliances",
            &crate::world::economy_data_publication::SparseMapProjection {
                base: &state.alliances,
                updates: &BTreeMap::new(),
                deletion: self.removed_alliance.as_ref(),
            },
        )
    }
}

impl WorldState {
    pub(crate) fn organization_risk_frozen(&self, id: &str) -> bool {
        self.world_rules.as_ref().is_some_and(|r| {
            r.organization_cases.values().any(|c| {
                c.organization_id == id
                    && c.phase != OrganizationProtectionPhaseV1::Resolved
                    && (c.disposition != OrganizationDispositionPhaseV1::None
                        || matches!(
                            c.phase,
                            OrganizationProtectionPhaseV1::Protected
                                | OrganizationProtectionPhaseV1::Limited
                        ))
            })
        })
    }
    /// Read-only, source-tick-bound explanation; never an executable snapshot.
    pub fn world_rule_status(&self, reader: Option<&str>) -> Option<serde_json::Value> {
        let rules = self.world_rules.as_ref()?;
        let orders=rules.agent_orders.iter().take(128).map(|(id,order)|{
            let blocker=if reader==Some(order.owner_id.as_str()){self.agent_industrial_order_blocker(order,self.time)}else{None};
            (id.clone(),serde_json::json!({"delivered":order.delivered_at.is_some(),"ready_at":order.ready_at,"blocker":blocker,"next_step":if order.delivered_at.is_some(){"query_delivery_receipt"}else if self.time<order.ready_at{"wait_for_production"}else if blocker.is_some(){"resolve_blocker_then_retry_same_order"}else{"await_authoritative_delivery"}}))
        }).collect::<BTreeMap<_,_>>();
        Some(
            serde_json::json!({"source_tick":self.time,"agent_orders":orders,"agent_order_count":rules.agent_orders.len(),"agent_orders_truncated":rules.agent_orders.len()>128,"supply_next_step":"inspect_original_authorization_batch_and_logistics_receipts","organization_next_step":"inspect_case_phase_request_status_scope_and_revision"}),
        )
    }
}

impl WorldRulesStateV1 {
    /// Public outcomes retain identity and phase; private demand/request bodies
    /// require the participating actor or the resource provider.
    pub fn for_reader(&self, reader: Option<&str>) -> Self {
        let mut value = self.clone();
        value
            .requests
            .retain(|_, r| Some(r.actor_id.as_str()) == reader);
        let mut visible_lineages = std::collections::BTreeSet::new();
        for auth in self.supply_authorizations.values() {
            for beneficiary in &auth.beneficiaries {
                if reader == Some(auth.provider_id.as_str()) || reader == Some(beneficiary.as_str())
                {
                    visible_lineages.insert(supply::lineage(auth, beneficiary));
                }
            }
        }
        for auth in value.supply_authorizations.values_mut() {
            let provider = reader == Some(auth.provider_id.as_str());
            auth.requests
                .retain(|id, _| provider || reader == Some(id.as_str()));
        }
        for batch in value.supply_batches.values_mut() {
            let provider = self
                .supply_authorizations
                .get(&batch.authorization_id)
                .is_some_and(|auth| reader == Some(auth.provider_id.as_str()));
            batch
                .demands
                .retain(|id, _| provider || reader == Some(id.as_str()));
            batch
                .allocation
                .retain(|id, _| provider || reader == Some(id.as_str()));
            batch
                .frozen_lineages
                .retain(|id, _| visible_lineages.contains(id));
        }
        value
            .supply_lineages
            .retain(|id, _| visible_lineages.contains(id));
        value.supply_shipments.retain(|_, s| {
            reader == Some(s.beneficiary_id.as_str())
                || self
                    .supply_authorizations
                    .get(&s.authorization_id)
                    .is_some_and(|auth| reader == Some(auth.provider_id.as_str()))
        });
        for case in value.organization_cases.values_mut() {
            let custodian = reader == Some(case.founder_id.as_str())
                || case
                    .grants
                    .values()
                    .any(|g| reader == Some(g.delegate_id.as_str()));
            case.requests
                .retain(|_, r| custodian || reader == Some(r.requester_id.as_str()));
        }
        value
    }
}
