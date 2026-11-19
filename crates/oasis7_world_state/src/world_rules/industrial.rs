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
        match self.validate_with_work_v1(&mut |_, _, _| Ok::<_, std::convert::Infallible>(())) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }
    pub fn validate_with_work_v1<E>(
        &self,
        work: &mut impl FnMut(u64, usize, usize) -> Result<(), E>,
    ) -> Result<Result<(), WorldError>, E> {
        super::readonly_order_validation::validate(self, work)
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
    /// Fixed original pre-production decision. Validation/source facts are
    /// DATA; only actual Current source owners may use this in a commit path.
    /// Callers preserve the lazy refund-source read after successful validate.
    pub fn delivery_prefix_after_validation_v1(
        &self,
        now: WorldTime,
        validation: Result<(), WorldError>,
        refund_source_current: Option<bool>,
    ) -> Option<String> {
        match self.delivery_prefix_after_validation_with_work_v1(
            now,
            validation,
            refund_source_current,
            &mut |_, _, _| Ok::<_, std::convert::Infallible>(()),
        ) {
            Ok(value) => value,
            Err(never) => match never {},
        }
    }
    pub fn delivery_prefix_after_validation_with_work_v1<E>(
        &self,
        now: WorldTime,
        validation: Result<(), WorldError>,
        refund_source_current: Option<bool>,
        work: &mut impl FnMut(u64, usize, usize) -> Result<(), E>,
    ) -> Result<Option<String>, E> {
        super::readonly_order_validation::prefix(self, now, validation, refund_source_current, work)
    }
    pub fn blocker(&self, state: &WorldState, now: WorldTime) -> Option<String> {
        match super::readonly_manufacturing::industrial_order_blocker_readonly_v1(state, self, now)
        {
            Ok(value) => value,
            Err(never) => match never {},
        }
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
/// Original primary/claim projection. This is business DATA, not a capability;
/// the Current consumer obtains the entire iterator from its private directory.
pub fn manufacturing_existing_targets_from_facts_v1<'a>(
    owner: &str,
    primary: super::industrial_eligibility_kernel::PrimaryProductionFactsV1<'_>,
    targets: impl IntoIterator<Item = &'a str>,
) -> Result<BTreeSet<String>, WorldError> {
    super::industrial_eligibility_kernel::validate_primary_production_facts_v1(primary, owner)?;
    let mut targets_out = BTreeSet::from([owner.to_string()]);
    targets_out.extend(targets.into_iter().map(str::to_owned));
    Ok(targets_out)
}
pub fn manufacturing_existing_targets(
    state: &WorldState,
    owner: &str,
) -> Result<BTreeSet<String>, WorldError> {
    manufacturing_existing_targets_from_facts_v1(
        owner,
        super::industrial_eligibility_kernel::PrimaryProductionFactsV1 {
            agent_exists: state.agents.contains_key(owner),
            starter_claim: state.starter_oc_claims.get(owner),
        },
        state
            .agent_claims
            .values()
            .filter(|c| c.claim_owner_id == owner)
            .map(|c| c.target_agent_id.as_str()),
    )
}
/// Fixed original union/creates-new order. In particular a reserved target must
/// not make creates_new_identity false before the existing set is inspected.
pub fn manufacturing_capacity_from_targets_v1<'a>(
    set: BTreeSet<String>,
    reserved: impl IntoIterator<Item = &'a str>,
    target: Option<&str>,
    new_order: bool,
    reputation_score: i64,
) -> Result<(), WorldError> {
    match manufacturing_capacity_from_targets_with_work_v1(
        set,
        reserved,
        target,
        new_order,
        reputation_score,
        &mut |_| Ok::<_, std::convert::Infallible>(()),
    ) {
        Ok(result) => result,
        Err(never) => match never {},
    }
}
pub fn manufacturing_capacity_from_targets_with_work_v1<'a, E>(
    mut set: BTreeSet<String>,
    reserved: impl IntoIterator<Item = &'a str>,
    target: Option<&str>,
    new_order: bool,
    reputation_score: i64,
    work: &mut impl FnMut(usize) -> Result<(), E>,
) -> Result<Result<(), WorldError>, E> {
    if let Some(target) = target {
        work(
            target
                .len()
                .saturating_add(1)
                .saturating_mul((set.len() + 1).ilog2() as usize + 2),
        )?;
    }
    let creates_new_identity = new_order || target.is_some_and(|id| !set.contains(id));
    for key in reserved {
        let levels = (set.len() + 1).ilog2() as usize + 2;
        work(
            512usize
                .saturating_mul(levels)
                .saturating_add(key.len().saturating_add(1).saturating_mul(levels * 2 + 4)),
        )?;
        set.insert(key.to_owned());
    }
    if let Some(target) = target {
        let levels = (set.len() + 1).ilog2() as usize + 2;
        work(
            512usize.saturating_mul(levels).saturating_add(
                target
                    .len()
                    .saturating_add(1)
                    .saturating_mul(levels * 2 + 4),
            ),
        )?;
        set.insert(target.into());
    }
    work(0)?;
    let count = set.len() + usize::from(new_order);
    let tier = crate::agent_claim_reputation_tier(reputation_score);
    if creates_new_identity && count > usize::from(crate::agent_claim_cap_for_tier(tier)) {
        return Ok(Err(invalid("manufacturing_acquisition_cap")));
    }
    Ok(Ok(()))
}
pub fn manufacturing_capacity_guard(
    state: &WorldState,
    owner: &str,
    target: Option<&str>,
    new_order: bool,
) -> Result<(), WorldError> {
    let set = manufacturing_existing_targets(state, owner)?;
    let reserved = manufacturing_reserved_targets(state, owner);
    manufacturing_capacity_from_targets_v1(
        set,
        reserved.iter().map(String::as_str),
        target,
        new_order,
        state.reputation_scores.get(owner).copied().unwrap_or(0),
    )
}

pub fn manufacturing_reserved_slots(state: &WorldState, factory: &str) -> u32 {
    state
        .world_rules
        .as_ref()
        .into_iter()
        .flat_map(|r| r.agent_orders.values())
        .filter(|o| manufacturing_order_reserves_slot_v1(o, factory))
        .count() as u32
}
fn legacy_production_facts<'a>(
    state: &'a WorldState,
    owner: &str,
    factory_id: &str,
    location: &str,
) -> super::industrial_eligibility_kernel::ProductionEligibilityFactsV1<'a> {
    use super::industrial_eligibility_kernel::*;
    let retired = state.retired_factory_ids.contains(factory_id);
    let blocked = state
        .gameplay_policy
        .blocked_agents
        .iter()
        .any(|id| id == owner);
    let factory = if retired || blocked {
        None
    } else {
        state.factories.get(factory_id)
    };
    let canonical =
        factory.and_then(|_| crate::builtin_factory_spec("factory.assembler.mk1", factory_id));
    let site = factory.and_then(|factory| state.factory_site_authorities.get(&factory.site_id));
    ProductionEligibilityFactsV1 {
        primary: PrimaryProductionFactsV1 {
            agent_exists: state.agents.contains_key(owner),
            starter_claim: state.starter_oc_claims.get(owner),
        },
        retired,
        blocked,
        factory: factory.map(|factory| FactoryProductionFactsV1 {
            builder_agent_id: &factory.builder_agent_id,
            builtin_spec_available: canonical.is_some(),
            spec_matches_builtin: canonical.as_ref().is_some_and(|spec| &factory.spec == spec),
            recipe_slots: u32::from(factory.spec.recipe_slots),
            durability_ppm: factory.durability_ppm,
            input_ledger: &factory.input_ledger,
            site_location_id: factory.site_location_id.as_deref(),
        }),
        site: site.map(|site| FactorySiteProductionFactsV1 {
            active: site.active,
            chunk_ready: site.chunk_ready,
            location_id: &site.location_id,
            owner_agent_id: &site.owner_agent_id,
            owner_authorized: site.authorized_agent_ids.iter().any(|id| id == owner),
        }),
        anchor_active: state
            .location_anchors
            .get(location)
            .map(|anchor| anchor.active),
        resolved_owner: None,
    }
}
fn refund_source_is_current(state: &WorldState, order: &AgentIndustrialOrderV1) -> bool {
    match super::readonly_manufacturing::refund_source_is_current_readonly_v1(state, order) {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

pub fn order_production_eligibility(
    state: &WorldState,
    order: &AgentIndustrialOrderV1,
) -> Result<(), WorldError> {
    use super::industrial_eligibility_kernel::*;
    validate_primary_production_facts_v1(
        PrimaryProductionFactsV1 {
            agent_exists: state.agents.contains_key(&order.owner_id),
            starter_claim: state.starter_oc_claims.get(&order.owner_id),
        },
        &order.owner_id,
    )?;
    let mut facts = legacy_production_facts(
        state,
        &order.owner_id,
        &order.factory_id,
        &order.quote.delivery_location_id,
    );
    production_eligibility_from_facts_v1(
        facts,
        &order.owner_id,
        &order.factory_id,
        &order.quote.delivery_location_id,
    )?;
    // Preserve the original lazy owner resolution after the input-source check.
    let resolved = facts
        .factory
        .filter(|factory| factory.input_ledger == &order.quote.input_ledger)
        .and_then(|_| state.resolved_agent_owner(&order.owner_id));
    facts.resolved_owner = resolved
        .as_ref()
        .map(|owner| ResolvedProductionOwnerFactsV1 {
            player_id: &owner.player_id,
            source_id: &owner.source_id,
            public_key: owner.public_key.as_deref(),
        });
    order_production_eligibility_from_facts_v1(facts, order)
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
        let owner = self.resolved_agent_owner(&order.owner_id);
        can_read_agent_industrial_order_from_owner_v1(order, reader, world_binding, owner.as_ref())
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
        match super::readonly_manufacturing::agent_manufacturing_quote_readonly_v1(
            self,
            owner,
            factory_id,
            quoted_at,
            world_binding,
        ) {
            Ok(result) => result,
            Err(never) => match never {},
        }
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
                        match advance_industrial_order_from_source_v1(
                            &mut order,
                            now,
                            IndustrialAdvanceEligibilityV1::LegacyWorld(state),
                        )? {
                            IndustrialAdvanceOutcomeV1::Updated => {}
                            IndustrialAdvanceOutcomeV1::Terminate(reason) => {
                                self.terminate_industrial(state, &mut order, reason, now)?
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
        mark_industrial_order_terminated_v1(order, reason);
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
        validate_industrial_refund_phase_v1(order)?;
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
            add_industrial_refund_materials_v1(order, &mut refunded)?;
            self.ledgers.insert(ledger.clone(), refunded);
            add_industrial_refund_electricity_v1(
                order,
                &mut self
                    .agents
                    .get_mut(&order.owner_id)
                    .ok_or_else(|| invalid("owner_missing"))?
                    .state
                    .resources,
            )?;
        }
        let mut account = state
            .main_token_balances
            .get(&order.owner_id)
            .cloned()
            .unwrap_or_default();
        add_industrial_refund_account_v1(order, &mut account)?;
        self.token_accounts.insert(order.owner_id.clone(), account);
        finish_industrial_refund_v1(order, now);
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

#[path = "industrial_advance.rs"]
mod advance;
pub use advance::{
    IndustrialAdvanceEligibilityV1, IndustrialAdvanceOutcomeV1, IndustrialAdvancePhaseV1,
    advance_industrial_order_from_source_v1, industrial_advance_phase_v1,
};

#[path = "industrial_refund.rs"]
mod refund;
pub use refund::{
    add_industrial_refund_account_v1, add_industrial_refund_electricity_v1,
    add_industrial_refund_materials_v1, finish_industrial_refund_v1,
    mark_industrial_order_terminated_v1, validate_industrial_refund_phase_v1,
};

/// Visibility only, shared by the original State wrapper and immutable Current
/// projection facts. It cannot create an order, grant a lease or apply an action.
pub fn can_read_agent_industrial_order_from_owner_v1(
    order: &AgentIndustrialOrderV1,
    reader: Option<&str>,
    world_binding: Option<&str>,
    owner: Option<&crate::ResolvedAgentOwnerV1>,
) -> bool {
    reader == Some(order.owner_id.as_str())
        && order.quote.world_binding.as_deref() == world_binding
        && owner.is_some_and(|owner| {
            owner.player_id == order.quote.owner_player_id
                && owner.source_id == order.quote.owner_source_id
                && owner.public_key.as_ref().is_some_and(|key| !key.is_empty())
        })
}

/// Original slot-reservation predicate, shared by mutation and readonly facts.
pub fn manufacturing_order_reserves_slot_v1(order: &AgentIndustrialOrderV1, factory: &str) -> bool {
    order.factory_id == factory
        && matches!(
            order.phase,
            AgentIndustrialPhaseV1::Reserved
                | AgentIndustrialPhaseV1::Producing
                | AgentIndustrialPhaseV1::ProductionBlocked
        )
}
