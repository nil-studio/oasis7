//! Sparse atomic projection for claim creation and upkeep settlement.

use serde::ser::SerializeStruct;
use std::collections::BTreeMap;

use super::super::agent_claims::{
    agent_claim_quote, auto_restricted_starter_claim_amount, split_agent_claim_spend,
    split_agent_claim_upfront_funding,
};
use super::super::gameplay_state::AgentClaimState;
use super::super::main_token::{
    MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL,
    MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL,
    RestrictedStarterClaimGrantStatus,
};
use super::super::{
    AgentCell, DomainEvent, MainTokenAccountBalance, MainTokenSupplyState, MaterialLedgerId,
    WorldError, WorldState, WorldTime,
};
use super::economy_data_publication::SparseMapProjection;

#[derive(Debug)]
pub(crate) struct PreparedAgentClaimEconomic {
    event: DomainEvent,
    actor: String,
    claim: AgentClaimState,
    account: MainTokenAccountBalance,
    supply: MainTokenSupplyState,
    treasury: BTreeMap<String, u64>,
    last_epoch: u64,
    agent: AgentCell,
    materials: BTreeMap<String, i64>,
}

impl PreparedAgentClaimEconomic {
    pub(crate) fn prepare(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        let actor = match event {
            DomainEvent::AgentClaimed {
                claimer_agent_id, ..
            }
            | DomainEvent::AgentClaimUpkeepSettled {
                claimer_agent_id, ..
            } => claimer_agent_id,
            _ => {
                return Err(invalid(
                    "claim economic preparation requires a supported event",
                ));
            }
        };
        let mut agent =
            state
                .agents
                .get(actor)
                .cloned()
                .ok_or_else(|| WorldError::AgentNotFound {
                    agent_id: actor.clone(),
                })?;
        let mut account = state.main_token_balances.get(actor).cloned();
        let mut supply = state.main_token_supply.clone();
        let mut treasury = BTreeMap::new();
        let (claim, last_epoch) = match event {
            DomainEvent::AgentClaimed {
                target_agent_id,
                reputation_tier,
                slot_index,
                activation_fee_amount,
                activation_fee_burn_amount,
                activation_fee_treasury_amount,
                claim_bond_amount,
                upfront_restricted_spent_amount,
                upfront_liquid_spent_amount,
                auto_issued_restricted_amount,
                auto_issued_restricted_source_treasury_bucket_id,
                claim_bond_locked_restricted_amount,
                claim_bond_locked_liquid_amount,
                upkeep_per_epoch,
                claimed_at_epoch,
                upkeep_paid_through_epoch,
                release_cooldown_epochs,
                grace_epochs,
                idle_warning_epochs,
                forced_idle_reclaim_epochs,
                forced_reclaim_penalty_bps,
                ..
            } => {
                if !state.agents.contains_key(target_agent_id) {
                    return Err(WorldError::AgentNotFound {
                        agent_id: target_agent_id.clone(),
                    });
                }
                if state.agent_claims.contains_key(target_agent_id) {
                    return Err(invalid(format!(
                        "agent claim already exists: target={target_agent_id}"
                    )));
                }
                let owned = state
                    .agent_claims
                    .values()
                    .filter(|c| c.claim_owner_id == *actor)
                    .count();
                let quote = agent_claim_quote(
                    state.reputation_scores.get(actor).copied().unwrap_or(0),
                    owned,
                )
                .map_err(|reason| {
                    invalid(format!(
                        "agent claim quote mismatch: owner={actor} reason={reason}"
                    ))
                })?;
                if quote.reputation_tier != *reputation_tier
                    || quote.slot_index != *slot_index
                    || quote.activation_fee_amount != *activation_fee_amount
                    || quote.activation_fee_burn_amount != *activation_fee_burn_amount
                    || quote.activation_fee_treasury_amount != *activation_fee_treasury_amount
                    || quote.claim_bond_amount != *claim_bond_amount
                    || quote.upkeep_per_epoch != *upkeep_per_epoch
                    || quote.release_cooldown_epochs != *release_cooldown_epochs
                    || quote.grace_epochs != *grace_epochs
                    || quote.idle_warning_epochs != *idle_warning_epochs
                    || quote.forced_idle_reclaim_epochs != *forced_idle_reclaim_epochs
                    || quote.forced_reclaim_penalty_bps != *forced_reclaim_penalty_bps
                {
                    return Err(invalid(format!(
                        "agent claim quote fields diverged: owner={actor} target={target_agent_id}"
                    )));
                }
                if *activation_fee_amount == 0 || *claim_bond_amount == 0 || *upkeep_per_epoch == 0
                {
                    return Err(invalid(format!(
                        "agent claim costs must be positive: target={target_agent_id}"
                    )));
                }
                if *upkeep_paid_through_epoch < *claimed_at_epoch {
                    return Err(invalid(format!(
                        "agent claim upkeep epoch mismatch: target={target_agent_id} claimed={claimed_at_epoch} paid_through={upkeep_paid_through_epoch}"
                    )));
                }
                let upfront = activation_fee_amount.checked_add(*claim_bond_amount).and_then(|v| v.checked_add(*upkeep_per_epoch)).ok_or_else(|| invalid(format!("agent claim upfront overflow: target={target_agent_id} activation={activation_fee_amount} bond={claim_bond_amount} upkeep={upkeep_per_epoch}")))?;
                let liquid = account.as_ref().map_or(0, |a| a.liquid_balance);
                let restricted = account
                    .as_ref()
                    .map_or(0, |a| a.restricted_starter_claim_balance);
                let auto = auto_restricted_starter_claim_amount(
                    *slot_index,
                    liquid,
                    restricted,
                    value(
                        state,
                        &treasury,
                        MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL,
                    ),
                    upfront,
                    state
                        .restricted_starter_claim_grants
                        .get(actor)
                        .is_some_and(|g| g.status == RestrictedStarterClaimGrantStatus::Issued),
                );
                let funding = split_agent_claim_upfront_funding(
                    *slot_index,
                    liquid,
                    restricted.saturating_add(auto),
                    *activation_fee_amount,
                    *claim_bond_amount,
                    *upkeep_per_epoch,
                )
                .map_err(invalid)?;
                let source = (auto > 0 && funding.claim_bond.restricted_amount > 0).then(|| {
                    MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL.to_string()
                });
                if auto != *auto_issued_restricted_amount
                    || source != *auto_issued_restricted_source_treasury_bucket_id
                {
                    return Err(invalid(format!(
                        "agent claim auto funding mismatch: target={target_agent_id} expected_amount={auto} actual_amount={auto_issued_restricted_amount} expected_bucket={source:?} actual_bucket={auto_issued_restricted_source_treasury_bucket_id:?}"
                    )));
                }
                if funding.upfront.restricted_amount != *upfront_restricted_spent_amount
                    || funding.upfront.liquid_amount != *upfront_liquid_spent_amount
                    || funding.claim_bond.restricted_amount != *claim_bond_locked_restricted_amount
                    || funding.claim_bond.liquid_amount != *claim_bond_locked_liquid_amount
                {
                    return Err(invalid(format!(
                        "agent claim funding split mismatch: target={target_agent_id} slot={slot_index} upfront_restricted={upfront_restricted_spent_amount} upfront_liquid={upfront_liquid_spent_amount} bond_restricted={claim_bond_locked_restricted_amount} bond_liquid={claim_bond_locked_liquid_amount}"
                    )));
                }
                if auto > 0 {
                    debit_treasury(
                        state,
                        &mut treasury,
                        MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL,
                        auto,
                    )?;
                    credit_restricted(&mut account, actor, auto)?;
                    increase_circulating(&mut supply, auto)?;
                }
                debit_restricted(&mut account, actor, *upfront_restricted_spent_amount)?;
                debit_liquid(&mut account, actor, *upfront_liquid_spent_amount)?;
                decrease_circulating(&mut supply, upfront)?;
                burn(&mut supply, *activation_fee_burn_amount)?;
                let ecosystem = activation_fee_treasury_amount.checked_add(*upkeep_per_epoch).ok_or_else(|| invalid(format!("agent claim treasury overflow: target={target_agent_id} activation_treasury={activation_fee_treasury_amount} upkeep={upkeep_per_epoch}")))?;
                add_treasury(
                    state,
                    &mut treasury,
                    MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL,
                    ecosystem,
                )?;
                (
                    AgentClaimState {
                        target_agent_id: target_agent_id.clone(),
                        claim_owner_id: actor.clone(),
                        reputation_tier: *reputation_tier,
                        slot_index: *slot_index,
                        activation_fee_amount: *activation_fee_amount,
                        activation_fee_burn_amount: *activation_fee_burn_amount,
                        activation_fee_treasury_amount: *activation_fee_treasury_amount,
                        claim_bond_amount: *claim_bond_amount,
                        locked_bond_amount: *claim_bond_amount,
                        upfront_restricted_spent_amount: *upfront_restricted_spent_amount,
                        upfront_liquid_spent_amount: *upfront_liquid_spent_amount,
                        claim_bond_locked_restricted_amount: *claim_bond_locked_restricted_amount,
                        claim_bond_locked_liquid_amount: *claim_bond_locked_liquid_amount,
                        claim_bond_restricted_source_treasury_bucket_id:
                            auto_issued_restricted_source_treasury_bucket_id.clone(),
                        upkeep_per_epoch: *upkeep_per_epoch,
                        release_cooldown_epochs: *release_cooldown_epochs,
                        grace_epochs: *grace_epochs,
                        idle_warning_epochs: *idle_warning_epochs,
                        forced_idle_reclaim_epochs: *forced_idle_reclaim_epochs,
                        forced_reclaim_penalty_bps: *forced_reclaim_penalty_bps,
                        claimed_at_epoch: *claimed_at_epoch,
                        upkeep_paid_through_epoch: *upkeep_paid_through_epoch,
                        delinquent_since_epoch: None,
                        grace_deadline_epoch: None,
                        release_requested_at_epoch: None,
                        release_ready_at_epoch: None,
                        idle_warning_emitted_at_epoch: None,
                    },
                    state
                        .agent_claim_last_processed_epoch
                        .max(*claimed_at_epoch),
                )
            }
            DomainEvent::AgentClaimUpkeepSettled {
                target_agent_id,
                settled_at_epoch,
                charged_epochs,
                amount,
                restricted_spent_amount,
                liquid_spent_amount,
                upkeep_paid_through_epoch,
                ..
            } => {
                let mut claim = state
                    .agent_claims
                    .get(target_agent_id)
                    .cloned()
                    .ok_or_else(|| {
                        invalid(format!("agent claim not found: target={target_agent_id}"))
                    })?;
                if claim.claim_owner_id != *actor {
                    return Err(invalid(format!(
                        "agent claim upkeep owner mismatch: target={target_agent_id} owner={} claimer={actor}",
                        claim.claim_owner_id
                    )));
                }
                if *charged_epochs == 0 || *amount == 0 {
                    return Err(invalid(format!(
                        "agent claim upkeep settlement must be positive: target={target_agent_id}"
                    )));
                }
                let expected = claim.upkeep_per_epoch.checked_mul(*charged_epochs).ok_or_else(|| invalid(format!("agent claim upkeep overflow: target={target_agent_id} upkeep={} epochs={charged_epochs}", claim.upkeep_per_epoch)))?;
                if expected != *amount {
                    return Err(invalid(format!(
                        "agent claim upkeep mismatch: target={target_agent_id} expected={expected} actual={amount}"
                    )));
                }
                let liquid = account.as_ref().map_or(0, |a| a.liquid_balance);
                let restricted = account
                    .as_ref()
                    .map_or(0, |a| a.restricted_starter_claim_balance);
                let funding =
                    split_agent_claim_spend(claim.slot_index, liquid, restricted, *amount)
                        .map_err(invalid)?;
                if funding.restricted_amount != *restricted_spent_amount
                    || funding.liquid_amount != *liquid_spent_amount
                {
                    return Err(invalid(format!(
                        "agent claim upkeep funding split mismatch: target={target_agent_id} slot={} restricted={restricted_spent_amount} liquid={liquid_spent_amount}",
                        claim.slot_index
                    )));
                }
                debit_restricted(&mut account, actor, *restricted_spent_amount)?;
                debit_liquid(&mut account, actor, *liquid_spent_amount)?;
                decrease_circulating(&mut supply, *amount)?;
                add_treasury(
                    state,
                    &mut treasury,
                    MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL,
                    *amount,
                )?;
                claim.upkeep_paid_through_epoch = *upkeep_paid_through_epoch;
                claim.delinquent_since_epoch = None;
                claim.grace_deadline_epoch = None;
                (
                    claim,
                    state
                        .agent_claim_last_processed_epoch
                        .max(*settled_at_epoch),
                )
            }
            _ => unreachable!(),
        };
        agent.last_active = now;
        Ok(Self {
            event: event.clone(),
            actor: actor.clone(),
            claim,
            account: account.expect("validated claim funding owns account"),
            supply,
            treasury,
            last_epoch,
            agent,
            materials: state
                .material_ledgers
                .get(&MaterialLedgerId::world())
                .filter(|m| !m.is_empty())
                .unwrap_or(&state.materials)
                .clone(),
        })
    }
    pub(crate) fn matches_event(&self, e: &DomainEvent) -> bool {
        &self.event == e
    }
    pub(crate) fn install_infallible(self, state: &mut WorldState) {
        state.agents.insert(self.actor.clone(), self.agent);
        state.main_token_balances.insert(self.actor, self.account);
        state.main_token_supply = self.supply;
        state.main_token_treasury_balances.extend(self.treasury);
        state
            .agent_claims
            .insert(self.claim.target_agent_id.clone(), self.claim);
        state.agent_claim_last_processed_epoch = self.last_epoch;
        state.materials = self.materials.clone();
        state
            .material_ledgers
            .insert(MaterialLedgerId::world(), self.materials);
    }
    pub(crate) fn serialize_agents<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let mut a = self.agent.clone();
        a.mailbox.push_back(self.event.clone());
        let u = BTreeMap::from([(self.actor.clone(), a)]);
        out.serialize_field(
            "agents",
            &SparseMapProjection {
                base: &state.agents,
                updates: &u,
                deletion: None,
            },
        )
    }
    pub(crate) fn serialize_materials<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field("materials", &self.materials)?;
        let u = BTreeMap::from([(MaterialLedgerId::world(), self.materials.clone())]);
        out.serialize_field(
            "material_ledgers",
            &SparseMapProjection {
                base: &state.material_ledgers,
                updates: &u,
                deletion: None,
            },
        )
    }
    pub(crate) fn serialize_claims<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let u = BTreeMap::from([(self.claim.target_agent_id.clone(), self.claim.clone())]);
        out.serialize_field(
            "agent_claims",
            &SparseMapProjection {
                base: &state.agent_claims,
                updates: &u,
                deletion: None,
            },
        )
    }
    pub(crate) fn serialize_last_epoch<S: SerializeStruct>(
        &self,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field("agent_claim_last_processed_epoch", &self.last_epoch)
    }
    pub(crate) fn serialize_token<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field("main_token_supply", &self.supply)?;
        let u = BTreeMap::from([(self.actor.clone(), self.account.clone())]);
        out.serialize_field(
            "main_token_balances",
            &SparseMapProjection {
                base: &state.main_token_balances,
                updates: &u,
                deletion: None,
            },
        )
    }
    pub(crate) fn serialize_treasury<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field(
            "main_token_treasury_balances",
            &SparseMapProjection {
                base: &state.main_token_treasury_balances,
                updates: &self.treasury,
                deletion: None,
            },
        )
    }
}
fn invalid(r: impl Into<String>) -> WorldError {
    WorldError::ResourceBalanceInvalid { reason: r.into() }
}
fn value(s: &WorldState, u: &BTreeMap<String, u64>, k: &str) -> u64 {
    u.get(k)
        .copied()
        .or_else(|| s.main_token_treasury_balances.get(k).copied())
        .unwrap_or(0)
}
fn debit_treasury(
    s: &WorldState,
    u: &mut BTreeMap<String, u64>,
    k: &str,
    a: u64,
) -> Result<(), WorldError> {
    let v = value(s, u, k);
    if v < a {
        return Err(invalid(format!(
            "main token treasury insufficient: bucket={k} balance={v} amount={a}"
        )));
    }
    u.insert(k.into(), v - a);
    Ok(())
}
fn add_treasury(
    s: &WorldState,
    u: &mut BTreeMap<String, u64>,
    k: &str,
    a: u64,
) -> Result<(), WorldError> {
    let n = value(s, u, k).checked_add(a).ok_or_else(|| {
        invalid(format!(
            "main token treasury overflow: bucket={k} amount={a}"
        ))
    })?;
    u.insert(k.into(), n);
    Ok(())
}
fn account<'a>(
    a: &'a mut Option<MainTokenAccountBalance>,
    id: &str,
) -> Result<&'a mut MainTokenAccountBalance, WorldError> {
    a.as_mut()
        .ok_or_else(|| invalid(format!("main token account not found: {id}")))
}
fn debit_liquid(
    a: &mut Option<MainTokenAccountBalance>,
    id: &str,
    n: u64,
) -> Result<(), WorldError> {
    let x = account(a, id)?;
    if x.liquid_balance < n {
        return Err(invalid(format!(
            "main token liquid balance insufficient: account={id} balance={} amount={n}",
            x.liquid_balance
        )));
    }
    x.liquid_balance -= n;
    Ok(())
}
fn debit_restricted(
    a: &mut Option<MainTokenAccountBalance>,
    id: &str,
    n: u64,
) -> Result<(), WorldError> {
    let x = account(a, id)?;
    if x.restricted_starter_claim_balance < n {
        return Err(invalid(format!(
            "restricted starter claim balance insufficient: account={id} balance={} amount={n}",
            x.restricted_starter_claim_balance
        )));
    }
    x.restricted_starter_claim_balance -= n;
    Ok(())
}
fn credit_restricted(
    a: &mut Option<MainTokenAccountBalance>,
    id: &str,
    n: u64,
) -> Result<(), WorldError> {
    let x = a.get_or_insert_with(|| MainTokenAccountBalance {
        account_id: id.into(),
        ..Default::default()
    });
    x.restricted_starter_claim_balance = x
        .restricted_starter_claim_balance
        .checked_add(n)
        .ok_or_else(|| {
            invalid(format!(
                "restricted starter claim credit overflow: account={id} current={} amount={n}",
                x.restricted_starter_claim_balance
            ))
        })?;
    Ok(())
}
fn decrease_circulating(s: &mut MainTokenSupplyState, n: u64) -> Result<(), WorldError> {
    if s.circulating_supply < n {
        return Err(invalid(format!(
            "main token circulating supply insufficient: circulating={} amount={n}",
            s.circulating_supply
        )));
    }
    s.circulating_supply -= n;
    Ok(())
}
fn increase_circulating(s: &mut MainTokenSupplyState, n: u64) -> Result<(), WorldError> {
    s.circulating_supply = s.circulating_supply.checked_add(n).ok_or_else(|| {
        invalid(format!(
            "main token circulating supply overflow: current={} amount={n}",
            s.circulating_supply
        ))
    })?;
    if s.circulating_supply > s.total_supply {
        return Err(invalid(format!(
            "main token circulating exceeds total supply: circulating={} total={}",
            s.circulating_supply, s.total_supply
        )));
    }
    Ok(())
}
fn burn(s: &mut MainTokenSupplyState, n: u64) -> Result<(), WorldError> {
    if s.total_supply < n {
        return Err(invalid(format!(
            "main token total supply insufficient for burn: total={} burn={n}",
            s.total_supply
        )));
    }
    s.total_supply -= n;
    s.total_burned = s.total_burned.checked_add(n).ok_or_else(|| {
        invalid(format!(
            "main token total_burned overflow: current={} burn={n}",
            s.total_burned
        ))
    })?;
    Ok(())
}
