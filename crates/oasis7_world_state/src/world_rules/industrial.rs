use super::*;
use crate::{AgentActivityV1, agent_state::AgentState};
use oasis7_agent_api::world_action::ResourceKind;
use std::collections::BTreeSet;

pub const AGENT_MANUFACTURING_WORK_TICKS: u32 = 12;
pub const AGENT_MANUFACTURING_POWER: i64 = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentIndustrialPhaseV1 {
    Reserved,
    Producing,
    ProductionBlocked,
    AwaitingDelivery,
    Delivered,
    Cancelled,
    Disposed,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentManufacturingQuoteV1 {
    pub policy_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_binding: Option<String>,
    pub recipe_id: String,
    pub owner_id: String,
    pub owner_player_id: String,
    pub owner_source_id: String,
    pub factory_id: String,
    pub quoted_at: WorldTime,
    pub expires_at: WorldTime,
    pub quote_digest: String,
    pub dependency_digest: String,
    pub delivery_location_id: String,
    pub inputs: BTreeMap<String, i64>,
    pub input_ledger: MaterialLedgerId,
    pub electricity: i64,
    pub claim_cost: crate::AgentClaimCostQuote,
    pub upfront_liquid: u64,
    pub liquid_after_reservation: u64,
    pub electricity_after_reservation: i64,
    pub maintenance_per_tick: i64,
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
    pub phase: AgentIndustrialPhaseV1,
    pub quote: AgentManufacturingQuoteV1,
    pub started_at: Option<WorldTime>,
    pub produced_at: Option<WorldTime>,
    pub blocked_at: Option<WorldTime>,
    pub completed_work_ticks: u32,
    pub last_progress_tick: Option<WorldTime>,
    pub terminal_reason: Option<String>,
    /// A terminal order may still own frozen refunds. These assets are never
    /// spendable by a replacement root principal and no longer occupy capacity.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub refund_pending: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refunded_at: Option<WorldTime>,
}
impl AgentIndustrialOrderV1 {
    pub fn validate(&self) -> Result<(), WorldError> {
        use AgentIndustrialPhaseV1 as P;
        let mut quote = self.quote.clone();
        let digest = std::mem::take(&mut quote.quote_digest);
        if self.policy_version != 1
            || quote.policy_version != 1
            || self.owner_id != quote.owner_id
            || self.factory_id != quote.factory_id
            || self.inputs != quote.inputs
            || quote.electricity != AGENT_MANUFACTURING_POWER
            || quote.claim_cost.slot_index < 2
            || quote.recipe_id != "recipe.assembler.agent_mk1"
            || crate::util::hash_json(&quote)? != digest
            || self.target_agent_id
                != format!("industrial:{}", crate::util::hash_json(&self.order_id)?)
        {
            return Err(invalid("manufacturing_recovery_incomplete"));
        }
        let phase_valid = match self.phase {
            P::Reserved => {
                self.started_at.is_none()
                    && self.completed_work_ticks == 0
                    && self.produced_at.is_none()
                    && self.delivered_at.is_none()
            }
            P::Producing | P::ProductionBlocked => {
                self.started_at.is_some()
                    && self.completed_work_ticks < 12
                    && self.produced_at.is_none()
                    && self.delivered_at.is_none()
            }
            P::AwaitingDelivery => {
                self.started_at.is_some()
                    && self.completed_work_ticks == 12
                    && self.produced_at.is_some()
                    && self.delivered_at.is_none()
            }
            P::Delivered => {
                self.started_at.is_some()
                    && self.completed_work_ticks == 12
                    && self.produced_at.is_some()
                    && self.delivered_at.is_some()
            }
            P::Cancelled => self.started_at.is_none() && self.completed_work_ticks == 0,
            P::Disposed => self.started_at.is_some(),
        };
        let refundable_terminal = matches!(self.phase, P::Cancelled | P::Disposed);
        if self.refund_pending && (!refundable_terminal || self.refunded_at.is_some())
            || self
                .refunded_at
                .is_some_and(|at| !refundable_terminal || at < self.confirmed_at)
        {
            return Err(invalid("manufacturing_refund_recovery_incomplete"));
        }
        if !phase_valid {
            return Err(invalid("manufacturing_recovery_incomplete"));
        }
        Ok(())
    }
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.phase,
            AgentIndustrialPhaseV1::Delivered
                | AgentIndustrialPhaseV1::Cancelled
                | AgentIndustrialPhaseV1::Disposed
        )
    }
    pub fn production_deadline(&self) -> Option<WorldTime> {
        self.started_at.map(|at| {
            at.saturating_add(36)
                .min(self.blocked_at.map_or(u64::MAX, |b| b.saturating_add(24)))
        })
    }
    pub fn blocker(&self, state: &WorldState, now: WorldTime) -> Option<String> {
        if let Err(error) = self.validate() {
            return Some(format!("{error:?}"));
        }
        if self.refund_pending {
            return Some(
                if refund_source_is_current(state, self) {
                    "refund_pending_authoritative_release"
                } else {
                    "restore_original_owner_source_for_frozen_refund"
                }
                .into(),
            );
        }
        if self.is_terminal() {
            return Some(format!("order_terminal:{:?}", self.phase));
        }
        if self.phase != AgentIndustrialPhaseV1::AwaitingDelivery {
            return Some(format!("production_phase:{:?}", self.phase));
        }
        let Some(produced_at) = self.produced_at else {
            return Some("manufacturing_state_inconsistent".into());
        };
        if self.completed_work_ticks != AGENT_MANUFACTURING_WORK_TICKS || self.started_at.is_none()
        {
            return Some("manufacturing_state_inconsistent".into());
        }
        if now >= produced_at.saturating_add(24) {
            return Some("delivery_expired".into());
        }
        if let Err(error) = order_production_eligibility(state, self) {
            return Some(format!("restore_production_authority:{error:?}"));
        }
        if let Err(error) =
            manufacturing_capacity_guard(state, &self.owner_id, Some(&self.target_agent_id), false)
        {
            return Some(format!("restore_acquisition_capacity:{error:?}"));
        }
        None
    }
}
pub fn manufacturing_reserved_targets(state: &WorldState, owner: &str) -> BTreeSet<String> {
    state
        .world_rules
        .as_ref()
        .into_iter()
        .flat_map(|r| r.agent_orders.values())
        .filter(|o| o.owner_id == owner && !o.is_terminal())
        .map(|o| o.target_agent_id.clone())
        .collect()
}
pub fn manufacturing_existing_targets(
    state: &WorldState,
    owner: &str,
) -> Result<BTreeSet<String>, WorldError> {
    if !state.agents.contains_key(owner) {
        return Err(invalid("primary_bound_agent_missing"));
    }
    let root = state
        .starter_oc_claims
        .get(owner)
        .ok_or_else(|| invalid("primary_bound_agent_required"))?;
    if root.agent_id != owner
        || root.player_id.is_empty()
        || root.public_key.as_ref().is_none_or(|key| key.is_empty())
    {
        return Err(invalid("primary_owner_source_unavailable"));
    }
    let mut targets = BTreeSet::from([owner.to_string()]);
    targets.extend(
        state
            .agent_claims
            .values()
            .filter(|c| c.claim_owner_id == owner)
            .map(|c| c.target_agent_id.clone()),
    );
    Ok(targets)
}
pub fn manufacturing_capacity_guard(
    state: &WorldState,
    owner: &str,
    target: Option<&str>,
    new_order: bool,
) -> Result<(), WorldError> {
    let mut set = manufacturing_existing_targets(state, owner)?;
    let creates_new_identity = new_order || target.is_some_and(|id| !set.contains(id));
    set.extend(manufacturing_reserved_targets(state, owner));
    if let Some(target) = target {
        set.insert(target.into());
    }
    let count = set.len() + usize::from(new_order);
    let tier = crate::agent_claim_reputation_tier(
        state.reputation_scores.get(owner).copied().unwrap_or(0),
    );
    if creates_new_identity && count > usize::from(crate::agent_claim_cap_for_tier(tier)) {
        return Err(invalid("manufacturing_acquisition_cap"));
    }
    Ok(())
}
pub fn manufacturing_reserved_slots(state: &WorldState, factory: &str) -> u32 {
    state
        .world_rules
        .as_ref()
        .into_iter()
        .flat_map(|r| r.agent_orders.values())
        .filter(|o| {
            o.factory_id == factory
                && matches!(
                    o.phase,
                    AgentIndustrialPhaseV1::Reserved
                        | AgentIndustrialPhaseV1::Producing
                        | AgentIndustrialPhaseV1::ProductionBlocked
                )
        })
        .count() as u32
}
fn production_eligibility(
    state: &WorldState,
    owner: &str,
    factory_id: &str,
    location: &str,
) -> Result<(), WorldError> {
    manufacturing_existing_targets(state, owner)?;
    if state.retired_factory_ids.contains(factory_id) {
        return Err(invalid("factory_retired"));
    }
    if state
        .gameplay_policy
        .blocked_agents
        .iter()
        .any(|id| id == owner)
    {
        return Err(invalid("production_authority_blocked"));
    }
    let f = state
        .factories
        .get(factory_id)
        .ok_or_else(|| invalid("factory_missing"))?;
    let canonical = crate::builtin_factory_spec("factory.assembler.mk1", factory_id)
        .ok_or_else(|| invalid("assembler_policy_unavailable"))?;
    if f.builder_agent_id != owner
        || f.spec != canonical
        || f.spec.recipe_slots < 2
        || f.durability_ppm <= 0
    {
        return Err(invalid("owned_working_assembler_required"));
    }
    let site = state
        .factory_site_authorities
        .get(&f.site_id)
        .ok_or_else(|| invalid("factory_site_authority_unavailable"))?;
    let anchor = state
        .location_anchors
        .get(location)
        .ok_or_else(|| invalid("factory_location_unavailable"))?;
    if !site.active
        || !site.chunk_ready
        || site.location_id != location
        || !anchor.active
        || f.site_location_id.as_deref() != Some(location)
        || !(site.owner_agent_id == owner || site.authorized_agent_ids.iter().any(|id| id == owner))
    {
        return Err(invalid("factory_authority_changed"));
    }
    Ok(())
}
fn refund_source_is_current(state: &WorldState, order: &AgentIndustrialOrderV1) -> bool {
    !state
        .gameplay_policy
        .blocked_agents
        .iter()
        .any(|id| id == &order.owner_id)
        && state
            .resolved_agent_owner(&order.owner_id)
            .is_some_and(|owner| {
                owner.player_id == order.quote.owner_player_id
                    && owner.source_id == order.quote.owner_source_id
                    && owner.public_key.as_ref().is_some_and(|key| !key.is_empty())
            })
}

pub fn order_production_eligibility(
    state: &WorldState,
    order: &AgentIndustrialOrderV1,
) -> Result<(), WorldError> {
    production_eligibility(
        state,
        &order.owner_id,
        &order.factory_id,
        &order.quote.delivery_location_id,
    )?;
    if state.factories[&order.factory_id].input_ledger != order.quote.input_ledger {
        return Err(invalid("manufacturing_input_source_changed"));
    }
    if !state
        .resolved_agent_owner(&order.owner_id)
        .is_some_and(|owner| {
            owner.player_id == order.quote.owner_player_id
                && owner.source_id == order.quote.owner_source_id
                && owner.public_key.as_ref().is_some_and(|key| !key.is_empty())
        })
    {
        return Err(invalid("manufacturing_root_owner_changed"));
    }
    Ok(())
}
impl WorldState {
    /// Private order bodies belong to the frozen principal, not merely a
    /// reusable Agent registry key. Source revocation leaves public status only.
    pub fn can_read_agent_industrial_order(
        &self,
        order: &AgentIndustrialOrderV1,
        reader: Option<&str>,
        world_binding: Option<&str>,
    ) -> bool {
        reader == Some(order.owner_id.as_str())
            && order.quote.world_binding.as_deref() == world_binding
            && self
                .resolved_agent_owner(&order.owner_id)
                .is_some_and(|owner| {
                    owner.player_id == order.quote.owner_player_id
                        && owner.source_id == order.quote.owner_source_id
                        && owner.public_key.as_ref().is_some_and(|key| !key.is_empty())
                })
    }
    pub fn agent_manufacturing_quote(
        &self,
        owner: &str,
        factory_id: &str,
        quoted_at: WorldTime,
    ) -> Result<AgentManufacturingQuoteV1, WorldError> {
        self.agent_manufacturing_quote_with_binding(owner, factory_id, quoted_at, None)
    }
    pub fn agent_manufacturing_quote_with_binding(
        &self,
        owner: &str,
        factory_id: &str,
        quoted_at: WorldTime,
        world_binding: Option<&str>,
    ) -> Result<AgentManufacturingQuoteV1, WorldError> {
        let f = self
            .factories
            .get(factory_id)
            .ok_or_else(|| invalid("factory_not_found"))?;
        let location = f
            .site_location_id
            .clone()
            .ok_or_else(|| invalid("factory_location_unavailable"))?;
        production_eligibility(self, owner, factory_id, &location)?;
        manufacturing_capacity_guard(self, owner, None, true)?;
        let pending: Vec<_> = self
            .world_rules
            .as_ref()
            .into_iter()
            .flat_map(|r| r.agent_orders.values())
            .filter(|o| o.owner_id == owner && !o.is_terminal())
            .collect();
        if pending.len() >= 2
            || self.world_rules.as_ref().is_some_and(|r| {
                r.agent_orders
                    .values()
                    .any(|o| o.factory_id == factory_id && !o.is_terminal())
            })
        {
            return Err(invalid("manufacturing_order_capacity"));
        }
        let jobs = self
            .pending_recipe_jobs
            .values()
            .filter(|j| j.factory_id == factory_id)
            .count()
            .max(f.production.active_jobs as usize);
        if jobs + 1 > usize::from(f.spec.recipe_slots) {
            return Err(invalid("factory_capacity_reserved"));
        }
        let inputs = BTreeMap::from([
            ("alloy_plate".into(), 8),
            ("motor_mk1".into(), 4),
            ("control_chip".into(), 4),
            ("sensor_pack".into(), 2),
            ("module_rack".into(), 1),
        ]);
        if f.input_ledger != MaterialLedgerId::agent(owner) {
            return Err(invalid("unsupported_input_authority_scope"));
        }
        let ledger = self
            .material_ledgers
            .get(&f.input_ledger)
            .ok_or_else(|| invalid("input_ledger_missing"))?;
        for (kind, amount) in &inputs {
            if ledger.get(kind).copied().unwrap_or(0) < *amount {
                return Err(invalid(format!("insufficient_material:{kind}")));
            }
        }
        let electricity = self.agents[owner]
            .state
            .resources
            .get(ResourceKind::Electricity);
        if electricity < AGENT_MANUFACTURING_POWER {
            return Err(invalid("insufficient_manufacturing_electricity"));
        }
        let scale = manufacturing_existing_targets(self, owner)?.len() + pending.len();
        let cost = crate::agent_claim_quote(
            self.reputation_scores.get(owner).copied().unwrap_or(0),
            scale,
        )
        .map_err(invalid)?;
        if cost.slot_index < 2 {
            return Err(invalid("additional_slot_required"));
        }
        let upfront = cost.activation_fee_amount + cost.claim_bond_amount + cost.upkeep_per_epoch;
        let account = self
            .main_token_balances
            .get(owner)
            .cloned()
            .unwrap_or_default();
        let liquid_after = account
            .liquid_balance
            .checked_sub(upfront)
            .ok_or_else(|| invalid("insufficient_liquid_claim_reservation"))?;
        // Tick depreciation and production audit timestamps do not change a
        // still-eligible quote. Eligibility and occupied capacity are checked
        // again above; freeze the factory's actual authority/specification.
        let factory_dependencies = (
            &f.factory_id,
            &f.site_id,
            &f.builder_agent_id,
            &f.spec,
            &f.input_ledger,
            &f.output_ledger,
            f.site_authority_revision,
            &f.site_location_id,
            f.location_anchor_revision,
            &f.construction_power_profile_key,
            f.construction_power_profile_revision,
            f.built_at,
            jobs,
        );
        let dependency_digest = crate::util::hash_json(&(
            factory_dependencies,
            &self.factory_site_authorities[&f.site_id],
            &self.location_anchors[&location],
            ledger,
            electricity,
            &account,
            &self.starter_oc_claims[owner],
            self.reputation_scores.get(owner),
            manufacturing_existing_targets(self, owner)?,
            manufacturing_reserved_targets(self, owner),
        ))?;
        let mut q = AgentManufacturingQuoteV1 {
            policy_version: 1,
            world_binding: world_binding.map(str::to_owned),
            recipe_id: "recipe.assembler.agent_mk1".into(),
            owner_id: owner.into(),
            owner_player_id: self.starter_oc_claims[owner].player_id.clone(),
            owner_source_id: self
                .resolved_agent_owner(owner)
                .ok_or_else(|| invalid("manufacturing_root_owner_unavailable"))?
                .source_id,
            factory_id: factory_id.into(),
            quoted_at,
            expires_at: quoted_at
                .checked_add(5)
                .ok_or_else(|| invalid("time_overflow"))?,
            quote_digest: String::new(),
            dependency_digest,
            delivery_location_id: location,
            inputs,
            input_ledger: f.input_ledger.clone(),
            electricity: AGENT_MANUFACTURING_POWER,
            claim_cost: cost,
            upfront_liquid: upfront,
            liquid_after_reservation: liquid_after,
            electricity_after_reservation: electricity - AGENT_MANUFACTURING_POWER,
            maintenance_per_tick: f.spec.maintenance_per_tick,
        };
        q.quote_digest = crate::util::hash_json(&q)?;
        Ok(q)
    }
}
impl PreparedWorldRule {
    pub fn industrial(
        &mut self,
        state: &WorldState,
        actor: &str,
        key: &str,
        command: &WorldRuleCommandV1,
        epoch: u64,
        now: WorldTime,
    ) -> Result<String, WorldError> {
        use AgentIndustrialPhaseV1 as P;
        let world_binding = match &self.event {
            DomainEvent::WorldRuleCommitted { world_binding, .. } => world_binding.clone(),
            _ => None,
        };
        match command {
            WorldRuleCommandV1::ConfirmAgentOrder {
                factory_id,
                quoted_at,
                quote_digest,
            } => {
                if *quoted_at > now || now >= quoted_at.saturating_add(5) {
                    return Err(invalid("manufacturing_quote_expired"));
                }
                let quote = state.agent_manufacturing_quote_with_binding(
                    actor,
                    factory_id,
                    *quoted_at,
                    world_binding.as_deref(),
                )?;
                if quote.quote_digest != *quote_digest {
                    return Err(invalid("manufacturing_quote_changed"));
                }
                if self
                    .rules
                    .agent_orders
                    .values()
                    .any(|o| o.quote.quote_digest == *quote_digest)
                {
                    return Err(invalid("manufacturing_quote_consumed"));
                }
                self.debit(
                    state,
                    &state.factories[factory_id].input_ledger,
                    &quote.inputs,
                )?;
                self.agents
                    .get_mut(actor)
                    .expect("actor")
                    .state
                    .resources
                    .remove(ResourceKind::Electricity, quote.electricity)
                    .map_err(|e| invalid(format!("electricity_reservation:{e:?}")))?;
                let mut account = state
                    .main_token_balances
                    .get(actor)
                    .cloned()
                    .unwrap_or_default();
                account.liquid_balance = quote.liquid_after_reservation;
                self.token_accounts.insert(actor.into(), account);
                let target = format!("industrial:{}", crate::util::hash_json(&key)?);
                if state.agents.contains_key(&target) {
                    return Err(invalid("target_identity_conflict"));
                }
                self.rules.agent_orders.insert(
                    key.into(),
                    AgentIndustrialOrderV1 {
                        order_id: key.into(),
                        owner_id: actor.into(),
                        factory_id: factory_id.clone(),
                        target_agent_id: target,
                        policy_version: 1,
                        inputs: quote.inputs.clone(),
                        confirmed_at: now,
                        ready_at: now.saturating_add(13),
                        delivered_at: None,
                        phase: P::Reserved,
                        quote,
                        started_at: None,
                        produced_at: None,
                        blocked_at: None,
                        completed_work_ticks: 0,
                        last_progress_tick: None,
                        terminal_reason: None,
                        refund_pending: false,
                        refunded_at: None,
                    },
                );
                Ok(key.into())
            }
            WorldRuleCommandV1::DeliverAgentOrder { order_id } => self.deliver_industrial(
                state,
                actor,
                order_id,
                epoch,
                now,
                world_binding.as_deref(),
            ),
            WorldRuleCommandV1::CancelAgentOrder { order_id }
            | WorldRuleCommandV1::DisposeAgentOrder { order_id }
            | WorldRuleCommandV1::AdvanceAgentOrder { order_id } => {
                let mut order = self
                    .rules
                    .agent_orders
                    .get(order_id)
                    .ok_or_else(|| invalid("order_not_found"))?
                    .clone();
                order.validate()?;
                if order.quote.world_binding != world_binding {
                    return Err(invalid("manufacturing_world_binding_mismatch"));
                }
                if actor != order.owner_id {
                    return Err(invalid("order_owner_required"));
                }
                if order.is_terminal() {
                    if order.refund_pending
                        && matches!(command, WorldRuleCommandV1::AdvanceAgentOrder { .. })
                    {
                        if !refund_source_is_current(state, &order) {
                            return Err(invalid("refund_original_source_recovery_required"));
                        }
                        self.release_industrial_refund(state, &mut order, now)?;
                        self.rules.agent_orders.insert(order_id.clone(), order);
                        return Ok(order_id.clone());
                    }
                    return Err(invalid("order_already_terminal"));
                }
                if matches!(
                    command,
                    WorldRuleCommandV1::CancelAgentOrder { .. }
                        | WorldRuleCommandV1::DisposeAgentOrder { .. }
                ) && !state.resolved_agent_owner(actor).is_some_and(|owner| {
                    owner.player_id == order.quote.owner_player_id
                        && owner.source_id == order.quote.owner_source_id
                }) {
                    return Err(invalid("manufacturing_root_owner_changed"));
                }
                match command {
                    WorldRuleCommandV1::CancelAgentOrder { .. } => {
                        if order.phase != P::Reserved {
                            return Err(invalid("production_already_started"));
                        }
                        self.terminate_industrial(state, &mut order, "owner_cancelled", now)?;
                    }
                    WorldRuleCommandV1::DisposeAgentOrder { .. } => {
                        if order.phase == P::Reserved {
                            return Err(invalid("cancel_reserved_order"));
                        }
                        self.terminate_industrial(state, &mut order, "owner_disposed", now)?;
                    }
                    _ => {
                        if order.phase == P::Reserved {
                            if now >= order.confirmed_at.saturating_add(10) {
                                self.terminate_industrial(state, &mut order, "start_expired", now)?;
                            } else {
                                order_production_eligibility(state, &order)?;
                                order.phase = P::Producing;
                                order.started_at = Some(now);
                                order.ready_at = now.saturating_add(12);
                                order.last_progress_tick = Some(now);
                            }
                        } else if order.phase == P::AwaitingDelivery {
                            if now < order.produced_at.expect("produced").saturating_add(24) {
                                return Err(invalid("delivery_pending"));
                            }
                            self.terminate_industrial(state, &mut order, "delivery_expired", now)?;
                        } else if now >= order.production_deadline().expect("started") {
                            self.terminate_industrial(
                                state,
                                &mut order,
                                "production_expired",
                                now,
                            )?;
                        } else if let Err(error) = order_production_eligibility(state, &order) {
                            order.phase = P::ProductionBlocked;
                            order.blocked_at.get_or_insert(now);
                            order.terminal_reason = Some(format!("production_blocked:{error:?}"));
                        } else {
                            if order.last_progress_tick.is_some_and(|tick| tick >= now) {
                                return Err(invalid("production_tick_already_counted"));
                            }
                            order.completed_work_ticks += 1;
                            order.last_progress_tick = Some(now);
                            order.ready_at =
                                now.saturating_add(u64::from(12 - order.completed_work_ticks));
                            order.phase = P::Producing;
                            order.terminal_reason = None;
                            if order.completed_work_ticks == 12 {
                                order.phase = P::AwaitingDelivery;
                                order.produced_at = Some(now);
                            }
                        }
                    }
                }
                self.rules.agent_orders.insert(order_id.clone(), order);
                Ok(order_id.clone())
            }
            _ => Err(invalid("unsupported_industrial_command")),
        }
    }
    fn terminate_industrial(
        &mut self,
        state: &WorldState,
        order: &mut AgentIndustrialOrderV1,
        reason: &str,
        now: WorldTime,
    ) -> Result<(), WorldError> {
        order.phase = if order.phase == AgentIndustrialPhaseV1::Reserved {
            AgentIndustrialPhaseV1::Cancelled
        } else {
            AgentIndustrialPhaseV1::Disposed
        };
        order.terminal_reason = Some(reason.into());
        order.refund_pending = true;
        if refund_source_is_current(state, order) {
            self.release_industrial_refund(state, order, now)?;
        }
        Ok(())
    }
    fn release_industrial_refund(
        &mut self,
        state: &WorldState,
        order: &mut AgentIndustrialOrderV1,
        now: WorldTime,
    ) -> Result<(), WorldError> {
        if !order.refund_pending
            || !matches!(
                order.phase,
                AgentIndustrialPhaseV1::Cancelled | AgentIndustrialPhaseV1::Disposed
            )
            || order.refunded_at.is_some()
        {
            return Err(invalid("manufacturing_refund_already_released_or_invalid"));
        }
        if !refund_source_is_current(state, order) {
            return Err(invalid("refund_original_source_recovery_required"));
        }
        if order.phase == AgentIndustrialPhaseV1::Cancelled {
            let ledger = &order.quote.input_ledger;
            let mut refunded = state
                .material_ledgers
                .get(ledger)
                .cloned()
                .unwrap_or_default();
            for (kind, amount) in &order.inputs {
                let value = refunded.entry(kind.clone()).or_default();
                *value = value
                    .checked_add(*amount)
                    .ok_or_else(|| invalid("refund_overflow"))?;
            }
            self.ledgers.insert(ledger.clone(), refunded);
            self.agents
                .get_mut(&order.owner_id)
                .ok_or_else(|| invalid("owner_missing"))?
                .state
                .resources
                .add(ResourceKind::Electricity, order.quote.electricity)
                .map_err(|error| invalid(format!("electricity_refund:{error:?}")))?;
        }
        let mut account = state
            .main_token_balances
            .get(&order.owner_id)
            .cloned()
            .unwrap_or_default();
        account.liquid_balance = account
            .liquid_balance
            .checked_add(order.quote.upfront_liquid)
            .ok_or_else(|| invalid("refund_overflow"))?;
        self.token_accounts.insert(order.owner_id.clone(), account);
        order.refund_pending = false;
        order.refunded_at = Some(now);
        Ok(())
    }
    fn deliver_industrial(
        &mut self,
        state: &WorldState,
        actor: &str,
        order_id: &str,
        epoch: u64,
        now: WorldTime,
        world_binding: Option<&str>,
    ) -> Result<String, WorldError> {
        let order = self
            .rules
            .agent_orders
            .get(order_id)
            .ok_or_else(|| invalid("order_not_found"))?
            .clone();
        if order.quote.world_binding.as_deref() != world_binding {
            return Err(invalid("manufacturing_world_binding_mismatch"));
        }
        if order.owner_id != actor {
            return Err(invalid("order_owner_required"));
        }
        if let Some(blocker) = order.blocker(state, now) {
            return Err(invalid(blocker));
        }
        if state.agents.contains_key(&order.target_agent_id) {
            return Err(invalid("target_identity_conflict"));
        }
        let q = order.quote.claim_cost.clone();
        let mut funded = state.clone();
        // Claim publication owns the actor cell; preserve admission activity in
        // that same sparse publication instead of losing the accepted action.
        if let Some(actor_cell) = self.agents.get(actor) {
            funded.agents.insert(actor.into(), actor_cell.clone());
        }
        let account = funded.main_token_balances.entry(actor.into()).or_default();
        account.liquid_balance = account
            .liquid_balance
            .checked_add(order.quote.upfront_liquid)
            .ok_or_else(|| invalid("funding_overflow"))?;
        let funding = crate::agent_claims::split_agent_claim_upfront_funding(
            q.slot_index,
            account.liquid_balance,
            0,
            q.activation_fee_amount,
            q.claim_bond_amount,
            q.upkeep_per_epoch,
        )
        .map_err(invalid)?;
        let auto = 0;
        let event = DomainEvent::AgentClaimed {
            acquisition_contract_version: 1,
            claimer_agent_id: actor.into(),
            target_agent_id: order.target_agent_id.clone(),
            reputation_tier: q.reputation_tier,
            slot_index: q.slot_index,
            activation_fee_amount: q.activation_fee_amount,
            activation_fee_burn_amount: q.activation_fee_burn_amount,
            activation_fee_treasury_amount: q.activation_fee_treasury_amount,
            claim_bond_amount: q.claim_bond_amount,
            upfront_restricted_spent_amount: funding.upfront.restricted_amount,
            upfront_liquid_spent_amount: funding.upfront.liquid_amount,
            auto_issued_restricted_amount: auto,
            auto_issued_restricted_source_treasury_bucket_id: (auto > 0
                && funding.claim_bond.restricted_amount > 0)
                .then(|| {
                    crate::MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL
                        .to_string()
                }),
            claim_bond_locked_restricted_amount: funding.claim_bond.restricted_amount,
            claim_bond_locked_liquid_amount: funding.claim_bond.liquid_amount,
            upkeep_per_epoch: q.upkeep_per_epoch,
            claimed_at_epoch: epoch,
            upkeep_paid_through_epoch: epoch,
            release_cooldown_epochs: q.release_cooldown_epochs,
            grace_epochs: q.grace_epochs,
            idle_warning_epochs: q.idle_warning_epochs,
            forced_idle_reclaim_epochs: q.forced_idle_reclaim_epochs,
            forced_reclaim_penalty_bps: q.forced_reclaim_penalty_bps,
        };
        let mut cell = AgentCell::new(
            AgentState::new(&order.target_agent_id, state.agents[actor].state.pos),
            now,
        );
        cell.state.resources = Default::default();
        cell.activity = Some(AgentActivityV1::idle(now));
        let mut claim=crate::world::agent_claim_economic_publication::PreparedAgentClaimEconomic::prepare_with_manufactured(&funded,&event,now,Some((order.target_agent_id.clone(),cell)))?;
        claim.set_routed_event(self.event.clone());
        self.claim = Some(claim);
        let delivered = self.rules.agent_orders.get_mut(order_id).expect("order");
        delivered.phase = AgentIndustrialPhaseV1::Delivered;
        delivered.delivered_at = Some(now);
        Ok(order.target_agent_id)
    }
}
