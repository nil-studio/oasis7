//! Authoritative v1 world-rule business state and sparse transaction preparation.
use crate::{AgentCell, DomainEvent, MaterialLedgerId, WorldError, WorldState, WorldTime};
use oasis7_agent_api::world_action::WorldRuleCommandV1;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod authority;
pub use authority::verify_world_rule_authority;

pub mod industrial;
mod industrial_completion;
pub mod industrial_eligibility_kernel;
pub mod industrial_work;
pub mod industrial_work_admission;
pub mod industrial_work_start;
mod industrial_work_terminate;
pub use industrial_completion::{
    validate_industrial_completion, validate_industrial_completion_destination_v1,
    validate_industrial_completion_selected_fields_v1,
};
pub use industrial_work_admission::industrial_ledger_access_from_records_v1;
pub mod recipe_substitution;
pub mod substitution_read;
use oasis7_agent_api::world_action::IndustrialWorkStateV1;
pub use recipe_substitution::*;
mod organization;
#[cfg(test)]
pub use organization::economic_contract_terms_hash;
pub use organization::{activity_catalog_root, activity_close_commitment};
mod supply;
pub use industrial::*;
pub use organization::*;
pub use supply::*;

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct WorldRulesStateV1 {
    pub requests: BTreeMap<String, WorldRuleReceiptV1>,
    pub governance_effects: BTreeMap<String, String>,
    pub organization_cases: BTreeMap<String, OrganizationCaseV1>,
    pub organization_estate: BTreeMap<String, OrganizationEstateDepositV1>,
    pub supply_authorizations: BTreeMap<String, SupplyAuthorizationV1>,
    pub supply_batches: BTreeMap<String, SupplyBatchV1>,
    pub supply_lineages: BTreeMap<String, SupplyLineageV1>,
    pub supply_shipments: BTreeMap<u64, SupplyShipmentV1>,
    #[serde(default)]
    pub organization_continuity: OrganizationContinuityStateV1,
    #[serde(default)]
    pub emergency_supply: EmergencySupplyStateV1,
    pub agent_orders: BTreeMap<String, AgentIndustrialOrderV1>,
    #[serde(default, skip_serializing_if = "industrial_work_is_empty")]
    pub industrial_work: IndustrialWorkStateV1,
    #[serde(default, skip_serializing_if = "substitution_is_empty")]
    pub substitution: SubstitutionStateV1,
}

fn industrial_work_is_empty(value: &IndustrialWorkStateV1) -> bool {
    value == &IndustrialWorkStateV1::default()
}
fn substitution_is_empty(value: &SubstitutionStateV1) -> bool {
    value == &SubstitutionStateV1::default()
}

pub struct WorldRulesContinuityProjection<'a> {
    pub base: &'a WorldRulesStateV1,
    pub organization_continuity: &'a OrganizationContinuityStateV1,
}

#[path="world_rules/projection_fields.rs"]
mod projection_fields;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldRuleReceiptV1 {
    pub request_id: String,
    pub actor_id: String,
    pub command: WorldRuleCommandV1,
    pub committed_at: WorldTime,
    pub result_id: String,
}
#[derive(Debug)]
pub struct PreparedWorldRule {
    pub event: DomainEvent,
    pub rules: WorldRulesStateV1,
    pub ledgers: BTreeMap<MaterialLedgerId, BTreeMap<String, i64>>,
    pub agents: BTreeMap<String, AgentCell>,
    pub locations: BTreeMap<String, crate::AgentLocationAuthorityV1>,
    pub token_accounts: BTreeMap<String, crate::MainTokenAccountBalance>,
    pub removed_alliance: Option<String>,
    pub industry: Option<crate::state::industry_transition::PreparedIndustryEvent>,
    pub claim: Option<crate::world::agent_claim_economic_publication::PreparedAgentClaimEconomic>,
}
impl PreparedWorldRule {
    pub fn prepare(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        let DomainEvent::WorldRuleCommitted {
            action_id,
            actor_id,
            request_id,
            command,
            governance_epoch,
            world_binding,
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
            token_accounts: BTreeMap::new(),
            locations: BTreeMap::new(),
            industry: None,
            claim: None,
        };
        let start_root=matches!(command,WorldRuleCommandV1::IndustrialWork{command}if matches!(&**command,oasis7_agent_api::world_action::IndustrialWorkCommandV1::StartRoot{..}));
        if start_root {
            match readonly_start_envelope::envelope(state,actor_id,request_id,command,world_binding.as_deref()) {Ok(result)=>if matches!(result?,readonly_start_envelope::StartEnvelopeDataV1::Replay){return Ok(next)},Err(never)=>match never{}}
        } else {
        if let Some(receipt) = next.rules.requests.get(&key) {
            if receipt.actor_id != *actor_id || receipt.command != *command {
                return Err(invalid("request_content_conflict"));
            }
            return Ok(next);
        }
        validate_manufactured_world_scope(state, actor_id, world_binding.as_deref())?;
        if serde_json::to_vec(command)?.len() > 65_536 {
            return Err(invalid("world_rule_command_too_large"));
        }
        if matches!(
            command,
            WorldRuleCommandV1::OpenOrganizationProtection { .. }
                | WorldRuleCommandV1::BeginOrganizationDissolution { .. }
                | WorldRuleCommandV1::AdvanceOrganization { .. }
                | WorldRuleCommandV1::GrantOrganizationScope { .. }
                | WorldRuleCommandV1::RevokeOrganizationScope { .. }
                | WorldRuleCommandV1::RequestOrganizationRecovery { .. }
                | WorldRuleCommandV1::ResolveOrganizationRecovery { .. }
                | WorldRuleCommandV1::DisposeOrganizationEstate { .. }
        ) {
            return Err(invalid("organization_continuity_upgrade_required"));
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
            && !matches!(event,DomainEvent::WorldRuleCommitted {action_id:0,command:WorldRuleCommandV1::EmergencySupply{command:oasis7_agent_api::world_action::EmergencySupplyCommandV1::Observe{..}|oasis7_agent_api::world_action::EmergencySupplyCommandV1::Close{..}},..})
            && !matches!(event,DomainEvent::WorldRuleCommitted {action_id:0,command:WorldRuleCommandV1::OrganizationContinuity{command:oasis7_agent_api::world_action::OrganizationContinuityCommandV1::FinalizeAvailabilityUnknown{..}|oasis7_agent_api::world_action::OrganizationContinuityCommandV1::OpenDormancyCase{..}|oasis7_agent_api::world_action::OrganizationContinuityCommandV1::PublishNotice{..}|oasis7_agent_api::world_action::OrganizationContinuityCommandV1::AdvancePhase{..}},..})
            && !matches!(event,DomainEvent::WorldRuleCommitted {action_id:0,command:WorldRuleCommandV1::AdvanceAgentOrder{..}|WorldRuleCommandV1::CancelAgentOrder{..}|WorldRuleCommandV1::DisposeAgentOrder{..},..})
            && !matches!(
                event,
                DomainEvent::WorldRuleCommitted {
                    action_id: 0,
                    command: WorldRuleCommandV1::OpenOrganizationProtection { .. },
                    ..
                }
            )
            && !matches!(event, DomainEvent::WorldRuleCommitted { action_id: 0, command, .. } if command.is_runtime_observation())
            && !matches!(event,DomainEvent::WorldRuleCommitted{action_id:0,command:WorldRuleCommandV1::AdvanceOrganization{case_id,..},..} if next.rules.organization_cases.get(case_id).is_some_and(|c|c.phase==OrganizationProtectionPhaseV1::Notice || c.phase==OrganizationProtectionPhaseV1::Limited && c.grants.values().all(|g|g.expires_at<=now||g.scopes.is_empty())))
        {
            return Err(invalid("actor_blocked"));
        }
        }
        if matches!(event,DomainEvent::WorldRuleCommitted {action_id,..} if *action_id>0) {
            next.agents
                .get_mut(actor_id)
                .expect("validated actor")
                .last_active = now;
        }
        if command.is_runtime_observation()
            && !matches!(event, DomainEvent::WorldRuleCommitted { action_id: 0, .. })
        {
            return Err(invalid("runtime_observation_is_system_only"));
        }
        let result_id = match command {
            WorldRuleCommandV1::IndustrialWork { command } => next.industrial_work_command(
                state,
                actor_id,
                &key,
                command,
                now,
                world_binding.as_deref(),
            )?,
            WorldRuleCommandV1::RecipeSubstitution { command } => next.recipe_substitution(
                state,
                actor_id,
                &key,
                command,
                now,
                world_binding.as_deref(),
            )?,
            WorldRuleCommandV1::MinimumService { command } => next.minimum_service(
                state,
                actor_id,
                &key,
                command,
                now,
                world_binding.as_deref().unwrap_or(""),
            )?,
            WorldRuleCommandV1::OrganizationContinuity { command } => next
                .organization_continuity(
                    state,
                    actor_id,
                    &key,
                    command,
                    now,
                    world_binding.as_deref().unwrap_or(""),
                    *action_id == 0,
                )?,
            WorldRuleCommandV1::EmergencySupply { command } => {
                if world_binding.is_none() || world_binding != &next.rules.emergency_supply.world_id
                {
                    return Err(invalid("emergency_supply_world_binding_mismatch"));
                }
                next.emergency_supply(state, actor_id, &key, command, now)?
            }
            WorldRuleCommandV1::ConfirmAgentOrder { .. }
            | WorldRuleCommandV1::DeliverAgentOrder { .. }
            | WorldRuleCommandV1::AdvanceAgentOrder { .. }
            | WorldRuleCommandV1::CancelAgentOrder { .. }
            | WorldRuleCommandV1::DisposeAgentOrder { .. } => {
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
            _ => next.organization(
                state,
                actor_id,
                &key,
                command,
                now,
                world_binding.as_deref().unwrap_or(""),
            )?,
        };
        if let Some(industry) = next.industry.as_mut() {
            industry.set_routing_event(event);
        }
        if let WorldRuleCommandV1::DeliverAgentOrder { order_id } = command {
            let order = &next.rules.agent_orders[order_id];
            if order.delivered_at == Some(now) {
                next.locations.insert(
                    order.target_agent_id.clone(),
                    crate::AgentLocationAuthorityV1 {
                        agent_id: order.target_agent_id.clone(),
                        location_id: order.quote.delivery_location_id.clone(),
                        active: true,
                        authority_revision: 1,
                        effective_at: now,
                    },
                );
            }
        }
        if let Some(id) = command.governance_proposal() {
            next.rules
                .governance_effects
                .insert(id.into(), result_id.clone());
        }
        let runtime_observation = command.is_runtime_observation()
            && matches!(event, DomainEvent::WorldRuleCommitted { action_id: 0, .. });
        if !runtime_observation {
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
        }
        Ok(next)
    }
    pub fn install(self, state: &mut WorldState) {
        if let Some(claim) = self.claim {
            claim.install_infallible(state);
        } else if let Some(industry) = self.industry {
            industry.install(state);
        } else {
            state.agents.extend(self.agents);
        }
        state.agent_location_authorities.extend(self.locations);
        state.main_token_balances.extend(self.token_accounts);
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
pub fn invalid(reason: impl Into<String>) -> WorldError {
    WorldError::ResourceBalanceInvalid {
        reason: reason.into(),
    }
}
pub fn validate_id(id: &str) -> Result<(), WorldError> {
    if id.is_empty() || id.len() > 128 || id.trim() != id {
        Err(invalid("invalid_identity"))
    } else {
        Ok(())
    }
}
pub fn request_key(actor: &str, request: &str) -> String {
    oasis7_agent_api::world_action::world_rule_request_key_v1(actor, request)
}

impl WorldState {
    /// Read-only order availability; no identity or claim is created by a quote.
    pub fn agent_industrial_order_blocker(
        &self,
        order: &AgentIndustrialOrderV1,
        now: WorldTime,
    ) -> Option<String> {
        order.blocker(self, now)
    }
}

impl PreparedWorldRule {
    pub fn serialize_agents<S: serde::ser::SerializeStruct>(
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
        if let Some(id) = self.event.agent_id()
            && let Some(agent) = agents.get_mut(id)
        {
            agent.mailbox.push_back(self.event.clone());
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
    pub fn serialize_materials<S: serde::ser::SerializeStruct>(
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
        || p.description != binding
    {
        return Err(invalid("bound_governance_approval_required"));
    }
    Ok(())
}

impl PreparedWorldRule {
    pub fn serialize_alliances<S: serde::ser::SerializeStruct>(
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
    pub fn organization_risk_frozen(&self, id: &str) -> bool {
        self.world_rules.as_ref().is_some_and(|r| {
            r.organization_continuity.cases.values().any(|c| {
                c.organization_id == id
                    && !matches!(
                        c.phase,
                        OrganizationContinuityPhaseV1::Resolved
                            | OrganizationContinuityPhaseV1::Cancelled
                    )
            }) || r.organization_cases.values().any(|c| {
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
        self.world_rule_status_with_binding(reader, None)
    }
    pub fn world_rule_status_with_binding(
        &self,
        reader: Option<&str>,
        world_binding: Option<&str>,
    ) -> Option<serde_json::Value> {
        match world_rule_status_readonly_v1(self,reader,world_binding){Ok(value)=>value,Err(never)=>match never{}}
    }

}

impl WorldRulesStateV1 {
    pub fn has_exact_request(
        &self,
        actor: &str,
        request: &str,
        command: &WorldRuleCommandV1,
    ) -> bool {
        self.requests
            .get(&request_key(actor, request))
            .is_some_and(|receipt| {
                receipt.actor_id == actor
                    && receipt.request_id == request
                    && receipt.command == *command
            })
    }

    /// Public outcomes retain identity and phase; private demand/request bodies
    /// require the participating actor or the resource provider.
    pub fn for_reader(&self, reader: Option<&str>) -> Self {
        let mut value = self.clone();
        value
            .agent_orders
            .retain(|_, o| reader == Some(o.owner_id.as_str()));
        value.requests.retain(|_, r| {
            Some(r.actor_id.as_str()) == reader
                && !matches!(
                    r.command,
                    WorldRuleCommandV1::IndustrialWork { .. }
                        | WorldRuleCommandV1::RecipeSubstitution { .. }
                )
        });
        // Without the canonical owner source, private industrial records stay closed.
        value.industrial_work = IndustrialWorkStateV1::default();
        value.substitution = SubstitutionStateV1::default();
        value.emergency_supply = self.emergency_supply.for_reader(reader);
        value.organization_continuity = self.organization_continuity.for_reader(reader);
        let visibility_source=readonly_visibility::LegacySupplyVisibilityV1(&self.supply_authorizations);
        let visibility=readonly_visibility::supply_visibility_plan_v1(&visibility_source,reader).unwrap();
        for auth in value.supply_authorizations.values_mut() {
            auth.requests.retain(|id,_|visibility.request(&visibility_source,&auth.provider_id,id,reader).unwrap());
        }
        for batch in value.supply_batches.values_mut() {
            let provider=visibility.provider(&visibility_source,&batch.authorization_id,reader).unwrap();
            batch
                .demands
                .retain(|id, _| visibility.beneficiary_request(&visibility_source,provider,id,reader).unwrap());
            batch
                .allocation
                .retain(|id, _| visibility.beneficiary_request(&visibility_source,provider,id,reader).unwrap());
            batch
                .frozen_lineages
                .retain(|id, _| visibility.lineage(&visibility_source,id).unwrap());
        }
        value
            .supply_lineages
            .retain(|id, _| visibility.lineage(&visibility_source,id).unwrap());
        value.supply_shipments.retain(|_,shipment|visibility.shipment(&visibility_source,shipment,reader).unwrap());
        for case in value.organization_cases.values_mut() {
            let custodian=visibility.case_custodian(&visibility_source,case,reader).unwrap();
            case.requests.retain(|_,request|visibility.case_request(&visibility_source,custodian,&request.requester_id,reader).unwrap());
        }
        value
    }
}

pub fn validate_manufactured_world_scope(
    state: &WorldState,
    agent: &str,
    binding: Option<&str>,
) -> Result<(), WorldError> {
    validate_manufactured_world_scope_from_order_v1(
        state.world_rules.as_ref().and_then(|rules| {
            rules
                .agent_orders
                .values()
                .find(|order| order.target_agent_id == agent)
        }),
        binding,
    )
}

pub fn validate_manufactured_world_scope_from_order_v1(
    order: Option<&AgentIndustrialOrderV1>,
    binding: Option<&str>,
) -> Result<(), WorldError> {
    if let Some(order) = order
        && order.quote.world_binding.as_deref() != binding
    {
        return Err(invalid("manufactured_actor_world_binding_mismatch"));
    }
    Ok(())
}

pub use industrial_work_start::{
    IndustrialOutputReservationRefV1, industrial_completion_output_map_heap_v1,
    validate_industrial_output_capacity_from_reservations_with_work_v1,
};
mod readonly_owner;
pub use readonly_owner::{IndustrialOwnerReadonlyFactsV1,is_manufactured_agent_readonly_v1,resolve_industrial_owner_readonly_v1};
mod readonly_manufacturing;
pub use readonly_manufacturing::{ManufacturingQuoteReadonlyFactsV1,agent_manufacturing_quote_readonly_v1,industrial_order_blocker_readonly_v1,order_production_eligibility_readonly_v1,refund_source_is_current_readonly_v1};
mod readonly_organization;
pub use readonly_organization::{OrganizationActivityReadonlyFactsV1,organization_activity_candidates_readonly_v1};
mod readonly_order_validation;
mod readonly_format;

#[path = "world_rules/supply_current_observation.rs"]
mod supply_current_observation;
pub use supply_current_observation::{
    EmergencyCurrentEvidenceV2, EmergencyCurrentObservationV2, current_evidence_source_ref_v2,
    current_observation_key_v2, validate_current_evidence_policy_v2,
    validate_current_observation_shape_v2,
};

mod readonly_industrial_data;
mod readonly_parent_consumption;
mod readonly_start_gates;
mod readonly_industrial_scratch;
mod readonly_recipe_tags;
mod readonly_industrial_consume;

pub use readonly_recipe_tags::compatible as recipe_preferred_tags_with_work_v1;
pub use readonly_industrial_consume::{IndustrialProductReadonlyFactsV1,industrial_effective_consume_readonly_v1};

pub use readonly_industrial_scratch::IndustrialScratchReadonlyFactsV1;

mod readonly_output_capacity;

mod readonly_industrial_start;
pub use readonly_industrial_start::{IndustrialStartReadonlyFactsV1,IndustrialStartInputPreviewDataV1,preview_industrial_start_inputs_readonly_v1};

pub(crate) use readonly_format::format as format_readonly_data_v1;

pub use readonly_format::format as format_readonly_projection_data_v1;

pub(crate) mod readonly_recipe_started;
pub use readonly_recipe_started::RecipeStartedReadonlyFactsV1;

mod readonly_start_recipe_overlay;
pub use readonly_industrial_start::{IndustrialStartRecipePreviewDataV1,preview_industrial_start_recipe_readonly_v1};

pub(crate) use readonly_recipe_started::factory_capacity_reservation_gate_v1 as readonly_recipe_factory_capacity_gate_v1;

mod readonly_start_envelope;
pub use readonly_start_envelope::{IndustrialStartEnvelopeReadonlyFactsV1,IndustrialStartRehearsalDataV1,preview_new_recipe_start_root_readonly_v1};

mod readonly_industrial_receipt;
pub use readonly_industrial_receipt::IndustrialReceiptReadonlyFactsV1;

mod readonly_start_industrial_overlay;

mod readonly_substitution_data;
mod readonly_substitution;
pub use readonly_substitution::{SubstitutionReadonlyFactsV1,preview_substitution_readonly_v1};

mod industrial_work_visibility;
pub use industrial_work_visibility::{industrial_root_reader_with_work_v1,industrial_receipt_reader_with_work_v1,industrial_binding_reader_with_work_v1,industrial_reference_equal_with_work_v1};

mod substitution_visibility;
pub use substitution_visibility::{current_rule_request_visible_with_work_v1,substitution_authority_visible_with_work_v1,substitution_decision_visible_with_work_v1};

mod readonly_status;
mod readonly_status_substitution;
mod readonly_status_legacy;
pub use readonly_status::{WorldRuleStatusReadonlyFactsV1,world_rule_status_readonly_v1};

pub use industrial_work_admission::industrial_owner_binding_with_work_v1;

mod readonly_visibility;
pub use readonly_visibility::*;
