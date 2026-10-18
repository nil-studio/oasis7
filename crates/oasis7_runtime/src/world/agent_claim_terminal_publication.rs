//! Sparse atomic projection for terminal agent-claim outcomes.

use serde::ser::SerializeStruct;
use std::collections::BTreeMap;

use super::super::agent_claims::split_agent_claim_bond_refund;
use super::super::main_token::{
    MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL, MAIN_TOKEN_TREASURY_BUCKET_SLASH,
    RestrictedStarterClaimGrantStatus, RestrictedStarterClaimRefundSink,
};
use super::super::{
    AgentCell, DomainEvent, MainTokenAccountBalance, MainTokenSupplyState, MaterialLedgerId,
    WorldError, WorldState, WorldTime,
};
use super::economy_data_publication::SparseMapProjection;

#[derive(Debug)]
pub(crate) struct PreparedAgentClaimTerminal {
    event: DomainEvent,
    actor: String,
    target: String,
    account: Option<MainTokenAccountBalance>,
    supply: MainTokenSupplyState,
    treasury: BTreeMap<String, u64>,
    last_epoch: u64,
    agent: AgentCell,
    materials: BTreeMap<String, i64>,
}

impl PreparedAgentClaimTerminal {
    pub(crate) fn prepare(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        let (actor, target, terminal_epoch) = match event {
            DomainEvent::AgentClaimReleased {
                claimer_agent_id,
                target_agent_id,
                released_at_epoch,
                ..
            } => (claimer_agent_id, target_agent_id, *released_at_epoch),
            DomainEvent::AgentClaimReclaimed {
                claimer_agent_id,
                target_agent_id,
                reclaimed_at_epoch,
                ..
            } => (claimer_agent_id, target_agent_id, *reclaimed_at_epoch),
            _ => {
                return Err(invalid(
                    "claim terminal preparation requires a supported event",
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
        let claim = state
            .agent_claims
            .get(target)
            .cloned()
            .ok_or_else(|| invalid(format!("agent claim not found: target={target}")))?;
        if claim.claim_owner_id != *actor {
            let operation = if matches!(event, DomainEvent::AgentClaimReleased { .. }) {
                "release"
            } else {
                "reclaim"
            };
            return Err(invalid(format!(
                "agent claim {operation} owner mismatch: target={target} owner={} claimer={actor}",
                claim.claim_owner_id
            )));
        }

        let mut account = state.main_token_balances.get(actor).cloned();
        let mut supply = state.main_token_supply.clone();
        let mut treasury = BTreeMap::new();
        match event {
            DomainEvent::AgentClaimReleased {
                refunded_bond_amount,
                refunded_bond_restricted_amount,
                refunded_bond_liquid_amount,
                refunded_bond_restricted_sink,
                refunded_bond_restricted_sink_bucket_id,
                ..
            } => {
                if *refunded_bond_amount != claim.locked_bond_amount {
                    return Err(invalid(format!(
                        "agent claim release refund mismatch: target={target} expected={} actual={refunded_bond_amount}",
                        claim.locked_bond_amount
                    )));
                }
                validate_refund(
                    state,
                    actor,
                    target,
                    &claim,
                    0,
                    *refunded_bond_amount,
                    *refunded_bond_restricted_amount,
                    *refunded_bond_liquid_amount,
                    refunded_bond_restricted_sink,
                    refunded_bond_restricted_sink_bucket_id,
                    "release",
                )?;
                refund(
                    state,
                    &mut account,
                    &mut supply,
                    &mut treasury,
                    actor,
                    target,
                    *refunded_bond_amount,
                    *refunded_bond_restricted_amount,
                    *refunded_bond_liquid_amount,
                    refunded_bond_restricted_sink,
                    refunded_bond_restricted_sink_bucket_id,
                    "release",
                    true,
                )?;
            }
            DomainEvent::AgentClaimReclaimed {
                upkeep_arrears_amount,
                collected_upkeep_amount,
                penalty_amount,
                refunded_bond_amount,
                refunded_bond_restricted_amount,
                refunded_bond_liquid_amount,
                refunded_bond_restricted_sink,
                refunded_bond_restricted_sink_bucket_id,
                ..
            } => {
                let collected = claim.locked_bond_amount.min(*upkeep_arrears_amount);
                let remaining = claim.locked_bond_amount.saturating_sub(collected);
                let penalty =
                    remaining.saturating_mul(u64::from(claim.forced_reclaim_penalty_bps)) / 10_000;
                let refunded = remaining.saturating_sub(penalty);
                if collected != *collected_upkeep_amount
                    || penalty != *penalty_amount
                    || refunded != *refunded_bond_amount
                {
                    return Err(invalid(format!(
                        "agent claim reclaim settlement mismatch: target={target} collected={collected_upkeep_amount} penalty={penalty_amount} refund={refunded_bond_amount}"
                    )));
                }
                validate_refund(
                    state,
                    actor,
                    target,
                    &claim,
                    collected_upkeep_amount.saturating_add(*penalty_amount),
                    *refunded_bond_amount,
                    *refunded_bond_restricted_amount,
                    *refunded_bond_liquid_amount,
                    refunded_bond_restricted_sink,
                    refunded_bond_restricted_sink_bucket_id,
                    "reclaim",
                )?;
                if *collected_upkeep_amount > 0 {
                    add_treasury(
                        state,
                        &mut treasury,
                        MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL,
                        *collected_upkeep_amount,
                    )?;
                }
                if *penalty_amount > 0 {
                    add_treasury(
                        state,
                        &mut treasury,
                        MAIN_TOKEN_TREASURY_BUCKET_SLASH,
                        *penalty_amount,
                    )?;
                }
                refund(
                    state,
                    &mut account,
                    &mut supply,
                    &mut treasury,
                    actor,
                    target,
                    *refunded_bond_amount,
                    *refunded_bond_restricted_amount,
                    *refunded_bond_liquid_amount,
                    refunded_bond_restricted_sink,
                    refunded_bond_restricted_sink_bucket_id,
                    "reclaim",
                    false,
                )?;
            }
            _ => unreachable!(),
        }
        agent.last_active = now;
        Ok(Self {
            event: event.clone(),
            actor: actor.clone(),
            target: target.clone(),
            account,
            supply,
            treasury,
            last_epoch: state.agent_claim_last_processed_epoch.max(terminal_epoch),
            agent,
            materials: state
                .material_ledgers
                .get(&MaterialLedgerId::world())
                .filter(|materials| !materials.is_empty())
                .unwrap_or(&state.materials)
                .clone(),
        })
    }

    pub(crate) fn matches_event(&self, event: &DomainEvent) -> bool {
        &self.event == event
    }

    pub(crate) fn install_infallible(self, state: &mut WorldState) {
        state.agent_claims.remove(&self.target);
        state.agents.insert(self.actor.clone(), self.agent);
        if let Some(account) = self.account {
            state.main_token_balances.insert(self.actor, account);
        }
        state.main_token_supply = self.supply;
        state.main_token_treasury_balances.extend(self.treasury);
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
        let mut agent = self.agent.clone();
        agent.mailbox.push_back(self.event.clone());
        let updates = BTreeMap::from([(self.actor.clone(), agent)]);
        out.serialize_field(
            "agents",
            &SparseMapProjection {
                base: &state.agents,
                updates: &updates,
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
        let updates = BTreeMap::from([(MaterialLedgerId::world(), self.materials.clone())]);
        out.serialize_field(
            "material_ledgers",
            &SparseMapProjection {
                base: &state.material_ledgers,
                updates: &updates,
                deletion: None,
            },
        )
    }

    pub(crate) fn serialize_claims<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let updates: BTreeMap<String, super::super::AgentClaimState> = BTreeMap::new();
        out.serialize_field(
            "agent_claims",
            &SparseMapProjection {
                base: &state.agent_claims,
                updates: &updates,
                deletion: Some(&self.target),
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
        let updates = self
            .account
            .clone()
            .map(|account| BTreeMap::from([(self.actor.clone(), account)]))
            .unwrap_or_default();
        out.serialize_field(
            "main_token_balances",
            &SparseMapProjection {
                base: &state.main_token_balances,
                updates: &updates,
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

#[allow(clippy::too_many_arguments)]
fn validate_refund(
    state: &WorldState,
    actor: &str,
    target: &str,
    claim: &super::super::AgentClaimState,
    consumed: u64,
    total: u64,
    restricted: u64,
    liquid: u64,
    sink: &RestrictedStarterClaimRefundSink,
    bucket: &str,
    operation: &str,
) -> Result<(), WorldError> {
    let expected = split_agent_claim_bond_refund(
        claim.claim_bond_locked_restricted_amount,
        claim.claim_bond_locked_liquid_amount,
        consumed,
    )
    .map_err(invalid)?;
    if expected.restricted_amount != restricted
        || expected.liquid_amount != liquid
        || restricted.saturating_add(liquid) != total
    {
        return Err(invalid(format!(
            "agent claim {operation} refund provenance mismatch: target={target} restricted={restricted} liquid={liquid} total={total}"
        )));
    }
    let (expected_sink, expected_bucket) = refund_sink(state, actor, claim);
    if *sink != expected_sink || bucket != expected_bucket {
        return Err(invalid(format!(
            "agent claim {operation} restricted refund sink mismatch: target={target} expected={expected_sink:?}/{expected_bucket} actual={sink:?}/{bucket}"
        )));
    }
    Ok(())
}

fn refund_sink(
    state: &WorldState,
    actor: &str,
    claim: &super::super::AgentClaimState,
) -> (RestrictedStarterClaimRefundSink, String) {
    if let Some(bucket) = &claim.claim_bond_restricted_source_treasury_bucket_id {
        return (
            RestrictedStarterClaimRefundSink::SourceTreasuryBucket,
            bucket.clone(),
        );
    }
    match state.restricted_starter_claim_grants.get(actor) {
        Some(grant)
            if matches!(
                grant.status,
                RestrictedStarterClaimGrantStatus::Expired
                    | RestrictedStarterClaimGrantStatus::Revoked
            ) =>
        {
            (
                RestrictedStarterClaimRefundSink::SourceTreasuryBucket,
                grant.source_treasury_bucket_id.clone(),
            )
        }
        _ => (
            RestrictedStarterClaimRefundSink::BeneficiaryRestrictedBalance,
            String::new(),
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn refund(
    state: &WorldState,
    account: &mut Option<MainTokenAccountBalance>,
    supply: &mut MainTokenSupplyState,
    treasury: &mut BTreeMap<String, u64>,
    actor: &str,
    target: &str,
    total: u64,
    restricted: u64,
    liquid: u64,
    sink: &RestrictedStarterClaimRefundSink,
    bucket: &str,
    operation: &str,
    apply_zero_refund: bool,
) -> Result<(), WorldError> {
    if total == 0 && !apply_zero_refund {
        return Ok(());
    }
    let mut circulating = liquid;
    match sink {
        RestrictedStarterClaimRefundSink::BeneficiaryRestrictedBalance => {
            credit_restricted(account, actor, restricted)?;
            circulating = circulating.checked_add(restricted).ok_or_else(|| {
                invalid(format!(
                    "agent claim {operation} circulating refund overflow: target={target} liquid={liquid} restricted={restricted}"
                ))
            })?;
        }
        RestrictedStarterClaimRefundSink::SourceTreasuryBucket => {
            add_treasury(state, treasury, bucket, restricted)?;
        }
    }
    credit_liquid(account, actor, liquid)?;
    increase_circulating(supply, circulating)
}

fn invalid(reason: impl Into<String>) -> WorldError {
    WorldError::ResourceBalanceInvalid {
        reason: reason.into(),
    }
}

fn add_treasury(
    state: &WorldState,
    updates: &mut BTreeMap<String, u64>,
    bucket: &str,
    amount: u64,
) -> Result<(), WorldError> {
    let current = updates
        .get(bucket)
        .copied()
        .or_else(|| state.main_token_treasury_balances.get(bucket).copied())
        .unwrap_or(0);
    let next = current.checked_add(amount).ok_or_else(|| {
        invalid(format!(
            "main token treasury overflow: bucket={bucket} amount={amount}"
        ))
    })?;
    updates.insert(bucket.to_string(), next);
    Ok(())
}

fn account<'a>(
    account: &'a mut Option<MainTokenAccountBalance>,
    actor: &str,
) -> &'a mut MainTokenAccountBalance {
    account.get_or_insert_with(|| MainTokenAccountBalance {
        account_id: actor.to_string(),
        ..Default::default()
    })
}

fn credit_liquid(
    balance: &mut Option<MainTokenAccountBalance>,
    actor: &str,
    amount: u64,
) -> Result<(), WorldError> {
    let account = account(balance, actor);
    account.liquid_balance = account.liquid_balance.checked_add(amount).ok_or_else(|| {
        invalid(format!(
            "main token liquid credit overflow: account={actor} current={} amount={amount}",
            account.liquid_balance
        ))
    })?;
    Ok(())
}

fn credit_restricted(
    balance: &mut Option<MainTokenAccountBalance>,
    actor: &str,
    amount: u64,
) -> Result<(), WorldError> {
    let account = account(balance, actor);
    account.restricted_starter_claim_balance = account
        .restricted_starter_claim_balance
        .checked_add(amount)
        .ok_or_else(|| {
            invalid(format!(
                "restricted starter claim credit overflow: account={actor} current={} amount={amount}",
                account.restricted_starter_claim_balance
            ))
        })?;
    Ok(())
}

fn increase_circulating(supply: &mut MainTokenSupplyState, amount: u64) -> Result<(), WorldError> {
    supply.circulating_supply = supply
        .circulating_supply
        .checked_add(amount)
        .ok_or_else(|| {
            invalid(format!(
                "main token circulating supply overflow: current={} amount={amount}",
                supply.circulating_supply
            ))
        })?;
    if supply.circulating_supply > supply.total_supply {
        return Err(invalid(format!(
            "main token circulating exceeds total supply: circulating={} total={}",
            supply.circulating_supply, supply.total_supply
        )));
    }
    Ok(())
}
