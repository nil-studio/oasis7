use serde::{Deserialize, Serialize};

pub const AGENT_CLAIM_BASE_ACTIVATION_FEE_AMOUNT: u64 = 100;
pub const AGENT_CLAIM_BASE_BOND_AMOUNT: u64 = 200;
pub const AGENT_CLAIM_BASE_UPKEEP_PER_EPOCH: u64 = 25;
pub const AGENT_CLAIM_ACTIVATION_FEE_BURN_BPS: u16 = 5_000;
pub const AGENT_CLAIM_RELEASE_COOLDOWN_EPOCHS: u64 = 3;
pub const AGENT_CLAIM_GRACE_EPOCHS: u64 = 2;
pub const AGENT_CLAIM_IDLE_WARNING_EPOCHS: u64 = 7;
pub const AGENT_CLAIM_IDLE_FORCED_RECLAIM_EPOCHS: u64 = 10;
pub const AGENT_CLAIM_FORCED_RECLAIM_PENALTY_BPS: u16 = 2_000;
pub const AGENT_CLAIM_TIER1_MIN_REPUTATION_SCORE: i64 = 10;
pub const AGENT_CLAIM_TIER2_MIN_REPUTATION_SCORE: i64 = 25;

const SLOT_MULTIPLIER_DENOMINATOR: u64 = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentClaimCostQuote {
    pub reputation_tier: u8,
    pub claim_cap: u8,
    pub slot_index: u8,
    pub activation_fee_amount: u64,
    pub activation_fee_burn_amount: u64,
    pub activation_fee_treasury_amount: u64,
    pub claim_bond_amount: u64,
    pub upkeep_per_epoch: u64,
    pub release_cooldown_epochs: u64,
    pub grace_epochs: u64,
    pub idle_warning_epochs: u64,
    pub forced_idle_reclaim_epochs: u64,
    pub forced_reclaim_penalty_bps: u16,
}

impl AgentClaimCostQuote {
    /// Read-only grant preview for this quoted slot and its complete upfront cost.
    /// Actual claim admission rechecks the current balances and grant state.
    pub fn preview_restricted_starter_grant(
        &self,
        liquid_balance: u64,
        restricted_balance: u64,
        liveops_pool_balance: u64,
        has_active_restricted_grant: bool,
    ) -> u64 {
        let total_upfront_amount = self.activation_fee_amount
            .saturating_add(self.claim_bond_amount)
            .saturating_add(self.upkeep_per_epoch);
        auto_restricted_starter_claim_amount(
            self.slot_index,
            liquid_balance,
            restricted_balance,
            liveops_pool_balance,
            total_upfront_amount,
            has_active_restricted_grant,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AgentClaimFundingSplit {
    pub restricted_amount: u64,
    pub liquid_amount: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AgentClaimUpfrontFundingBreakdown {
    pub upfront: AgentClaimFundingSplit,
    pub claim_bond: AgentClaimFundingSplit,
}

pub fn agent_claim_reputation_tier(reputation_score: i64) -> u8 {
    if reputation_score >= AGENT_CLAIM_TIER2_MIN_REPUTATION_SCORE {
        2
    } else if reputation_score >= AGENT_CLAIM_TIER1_MIN_REPUTATION_SCORE {
        1
    } else {
        0
    }
}

pub fn agent_claim_cap_for_tier(reputation_tier: u8) -> u8 {
    match reputation_tier {
        0 => 1,
        1 => 2,
        _ => 3,
    }
}

pub fn agent_claim_quote(
    reputation_score: i64,
    owned_claim_count: usize,
) -> Result<AgentClaimCostQuote, String> {
    let reputation_tier = agent_claim_reputation_tier(reputation_score);
    let claim_cap = agent_claim_cap_for_tier(reputation_tier);
    let next_slot_index = owned_claim_count
        .checked_add(1)
        .ok_or_else(|| "agent claim slot overflow".to_string())?;
    if next_slot_index > usize::from(claim_cap) {
        return Err(format!(
            "agent claim cap exceeded: owned={} cap={}",
            owned_claim_count, claim_cap
        ));
    }

    let slot_index =
        u8::try_from(next_slot_index).map_err(|_| "agent claim slot exceeds u8".to_string())?;
    let multiplier_numerator = slot_multiplier_numerator(slot_index)?;
    let activation_fee_amount =
        scale_amount_ceil(AGENT_CLAIM_BASE_ACTIVATION_FEE_AMOUNT, multiplier_numerator)?;
    let claim_bond_amount = scale_amount_ceil(AGENT_CLAIM_BASE_BOND_AMOUNT, multiplier_numerator)?;
    let upkeep_per_epoch =
        scale_amount_ceil(AGENT_CLAIM_BASE_UPKEEP_PER_EPOCH, multiplier_numerator)?;
    let activation_fee_burn_amount = activation_fee_amount
        .saturating_mul(u64::from(AGENT_CLAIM_ACTIVATION_FEE_BURN_BPS))
        / 10_000;
    let activation_fee_treasury_amount = activation_fee_amount
        .checked_sub(activation_fee_burn_amount)
        .ok_or_else(|| "agent claim activation fee split underflow".to_string())?;

    Ok(AgentClaimCostQuote {
        reputation_tier,
        claim_cap,
        slot_index,
        activation_fee_amount,
        activation_fee_burn_amount,
        activation_fee_treasury_amount,
        claim_bond_amount,
        upkeep_per_epoch,
        release_cooldown_epochs: AGENT_CLAIM_RELEASE_COOLDOWN_EPOCHS,
        grace_epochs: AGENT_CLAIM_GRACE_EPOCHS,
        idle_warning_epochs: AGENT_CLAIM_IDLE_WARNING_EPOCHS,
        forced_idle_reclaim_epochs: AGENT_CLAIM_IDLE_FORCED_RECLAIM_EPOCHS,
        forced_reclaim_penalty_bps: AGENT_CLAIM_FORCED_RECLAIM_PENALTY_BPS,
    })
}

fn slot_multiplier_numerator(slot_index: u8) -> Result<u64, String> {
    match slot_index {
        1 => Ok(10),
        2 => Ok(15),
        3 => Ok(20),
        _ => Err(format!("unsupported agent claim slot index: {slot_index}")),
    }
}

fn scale_amount_ceil(base: u64, multiplier_numerator: u64) -> Result<u64, String> {
    let scaled = base
        .checked_mul(multiplier_numerator)
        .ok_or_else(|| format!("agent claim amount overflow: base={base}"))?;
    scaled
        .checked_add(SLOT_MULTIPLIER_DENOMINATOR - 1)
        .ok_or_else(|| format!("agent claim amount ceil overflow: scaled={scaled}"))
        .map(|value| value / SLOT_MULTIPLIER_DENOMINATOR)
}

pub(crate) fn agent_claim_eligible_balance(
    slot_index: u8,
    liquid_balance: u64,
    restricted_balance: u64,
) -> u64 {
    match slot_index {
        1 => liquid_balance.saturating_add(restricted_balance),
        _ => liquid_balance,
    }
}

pub(crate) fn auto_restricted_starter_claim_amount(
    slot_index: u8,
    liquid_balance: u64,
    restricted_balance: u64,
    liveops_pool_balance: u64,
    total_upfront_amount: u64,
    has_active_restricted_grant: bool,
) -> u64 {
    if slot_index != 1 || has_active_restricted_grant || restricted_balance > 0 {
        return 0;
    }
    let eligible_without_auto =
        agent_claim_eligible_balance(slot_index, liquid_balance, restricted_balance);
    if eligible_without_auto >= total_upfront_amount {
        return 0;
    }
    liveops_pool_balance.min(total_upfront_amount.saturating_sub(eligible_without_auto))
}

pub(crate) fn split_agent_claim_upfront_funding(
    slot_index: u8,
    liquid_balance: u64,
    restricted_balance: u64,
    activation_fee_amount: u64,
    claim_bond_amount: u64,
    upkeep_per_epoch: u64,
) -> Result<AgentClaimUpfrontFundingBreakdown, String> {
    let upfront_amount = activation_fee_amount
        .checked_add(claim_bond_amount)
        .and_then(|value| value.checked_add(upkeep_per_epoch))
        .ok_or_else(|| {
            format!(
                "agent claim upfront overflow: slot={} activation={} bond={} upkeep={}",
                slot_index, activation_fee_amount, claim_bond_amount, upkeep_per_epoch
            )
        })?;
    let upfront = split_agent_claim_spend(
        slot_index,
        liquid_balance,
        restricted_balance,
        upfront_amount,
    )?;
    let restricted_after_activation = upfront
        .restricted_amount
        .saturating_sub(activation_fee_amount);
    let claim_bond_restricted_amount = restricted_after_activation.min(claim_bond_amount);
    let claim_bond_liquid_amount = claim_bond_amount.saturating_sub(claim_bond_restricted_amount);
    Ok(AgentClaimUpfrontFundingBreakdown {
        upfront,
        claim_bond: AgentClaimFundingSplit {
            restricted_amount: claim_bond_restricted_amount,
            liquid_amount: claim_bond_liquid_amount,
        },
    })
}

pub(crate) fn split_agent_claim_spend(
    slot_index: u8,
    liquid_balance: u64,
    restricted_balance: u64,
    amount: u64,
) -> Result<AgentClaimFundingSplit, String> {
    if amount == 0 {
        return Ok(AgentClaimFundingSplit {
            restricted_amount: 0,
            liquid_amount: 0,
        });
    }
    match slot_index {
        1 => {
            let eligible =
                agent_claim_eligible_balance(slot_index, liquid_balance, restricted_balance);
            if eligible < amount {
                return Err(format!(
                    "slot-1 eligible claim balance insufficient: eligible={} liquid={} restricted={} amount={}",
                    eligible, liquid_balance, restricted_balance, amount
                ));
            }
            let restricted_amount = restricted_balance.min(amount);
            Ok(AgentClaimFundingSplit {
                restricted_amount,
                liquid_amount: amount.saturating_sub(restricted_amount),
            })
        }
        2 | 3 => {
            if liquid_balance < amount {
                return Err(format!(
                    "slot-{} liquid claim balance insufficient: liquid={} amount={}",
                    slot_index, liquid_balance, amount
                ));
            }
            Ok(AgentClaimFundingSplit {
                restricted_amount: 0,
                liquid_amount: amount,
            })
        }
        _ => Err(format!("unsupported agent claim slot index: {slot_index}")),
    }
}

pub(crate) fn split_agent_claim_bond_refund(
    claim_bond_locked_restricted_amount: u64,
    claim_bond_locked_liquid_amount: u64,
    consumed_bond_amount: u64,
) -> Result<AgentClaimFundingSplit, String> {
    let locked_bond_amount = claim_bond_locked_restricted_amount
        .checked_add(claim_bond_locked_liquid_amount)
        .ok_or_else(|| {
            format!(
                "agent claim bond provenance overflow: restricted={} liquid={}",
                claim_bond_locked_restricted_amount, claim_bond_locked_liquid_amount
            )
        })?;
    if consumed_bond_amount > locked_bond_amount {
        return Err(format!(
            "agent claim consumed bond exceeds locked amount: consumed={} locked={}",
            consumed_bond_amount, locked_bond_amount
        ));
    }
    let restricted_consumed = claim_bond_locked_restricted_amount.min(consumed_bond_amount);
    let liquid_consumed = consumed_bond_amount.saturating_sub(restricted_consumed);
    Ok(AgentClaimFundingSplit {
        restricted_amount: claim_bond_locked_restricted_amount.saturating_sub(restricted_consumed),
        liquid_amount: claim_bond_locked_liquid_amount.saturating_sub(liquid_consumed),
    })
}
