use super::*;
use crate::{AgentActivityV1, agent_state::AgentState};

impl PreparedWorldRule {
    pub(super) fn industrial(
        &mut self,
        state: &WorldState,
        actor: &str,
        key: &str,
        command: &WorldRuleCommandV1,
        epoch: u64,
        now: WorldTime,
    ) -> Result<String, WorldError> {
        match command {
            WorldRuleCommandV1::ConfirmAgentOrder { factory_id } => {
                let factory = state
                    .factories
                    .get(factory_id)
                    .ok_or_else(|| invalid("factory_not_found"))?;
                if factory.builder_agent_id != actor
                    || !factory.spec.tags.iter().any(|tag| tag == "assembler")
                    || factory.durability_ppm <= 0
                {
                    return Err(invalid("owned_working_assembler_required"));
                }
                if factory.production.active_jobs > 0
                    || state
                        .pending_recipe_jobs
                        .values()
                        .any(|j| j.factory_id == *factory_id)
                    || self
                        .rules
                        .agent_orders
                        .values()
                        .any(|o| o.factory_id == *factory_id && o.delivered_at.is_none())
                {
                    return Err(invalid("factory_capacity_reserved"));
                }
                let owned = state
                    .agent_claims
                    .values()
                    .filter(|c| c.claim_owner_id == actor)
                    .count();
                if owned == 0 {
                    return Err(invalid("primary_agent_claim_required"));
                }
                crate::agent_claim_quote(
                    state.reputation_scores.get(actor).copied().unwrap_or(0),
                    owned,
                )
                .map_err(invalid)?;
                let inputs = BTreeMap::from([
                    ("alloy_plate".into(), 12),
                    ("control_chip".into(), 2),
                    ("motor_mk1".into(), 4),
                    ("sensor_pack".into(), 2),
                ]);
                self.debit(state, &MaterialLedgerId::agent(actor), &inputs)?;
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
                        inputs,
                        confirmed_at: now,
                        ready_at: now
                            .checked_add(120)
                            .ok_or_else(|| invalid("time_overflow"))?,
                        delivered_at: None,
                    },
                );
                Ok(key.into())
            }
            WorldRuleCommandV1::DeliverAgentOrder { order_id } => {
                let order = self
                    .rules
                    .agent_orders
                    .get(order_id)
                    .ok_or_else(|| invalid("order_not_found"))?
                    .clone();
                if order.owner_id != actor {
                    return Err(invalid("order_owner_required"));
                }
                if let Some(blocker) = state.agent_industrial_order_blocker(&order, now) {
                    return Err(invalid(blocker));
                }
                if state.agents.contains_key(&order.target_agent_id) {
                    return Err(invalid("target_identity_conflict"));
                }
                let (q, funding, auto) = funding_for_order(state, actor)?;
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
                cell.activity = Some(AgentActivityV1::idle(now));
                let mut claim=crate::world::agent_claim_economic_publication::PreparedAgentClaimEconomic::prepare_with_manufactured(state,&event,now,Some((order.target_agent_id.clone(),cell)))?;
                claim.set_routed_event(self.event.clone());
                self.claim = Some(claim);
                self.rules
                    .agent_orders
                    .get_mut(order_id)
                    .expect("validated order")
                    .delivered_at = Some(now);
                Ok(order.target_agent_id)
            }
            _ => Err(invalid("unsupported_industrial_command")),
        }
    }
}

pub(super) fn funding_for_order(
    state: &WorldState,
    actor: &str,
) -> Result<
    (
        crate::AgentClaimCostQuote,
        crate::agent_claims::AgentClaimUpfrontFundingBreakdown,
        u64,
    ),
    WorldError,
> {
    let owned = state
        .agent_claims
        .values()
        .filter(|c| c.claim_owner_id == actor)
        .count();
    let q = crate::agent_claim_quote(
        state.reputation_scores.get(actor).copied().unwrap_or(0),
        owned,
    )
    .map_err(invalid)?;
    let account = state
        .main_token_balances
        .get(actor)
        .cloned()
        .unwrap_or_default();
    let auto = q.preview_restricted_starter_grant(
        account.liquid_balance,
        account.restricted_starter_claim_balance,
        state
            .main_token_treasury_balances
            .get(crate::MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL)
            .copied()
            .unwrap_or(0),
        state
            .restricted_starter_claim_grants
            .get(actor)
            .is_some_and(|g| g.status == crate::RestrictedStarterClaimGrantStatus::Issued),
    );
    let funding = crate::agent_claims::split_agent_claim_upfront_funding(
        q.slot_index,
        account.liquid_balance,
        account
            .restricted_starter_claim_balance
            .saturating_add(auto),
        q.activation_fee_amount,
        q.claim_bond_amount,
        q.upkeep_per_epoch,
    )
    .map_err(invalid)?;
    Ok((q, funding, auto))
}
