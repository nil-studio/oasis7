use super::*;
use crate::events::MainTokenFeeKind;
use crate::main_token::{
    MAIN_TOKEN_TREASURY_BUCKET_GAS_FEE, MAIN_TOKEN_TREASURY_BUCKET_MODULE_FEE,
    MAIN_TOKEN_TREASURY_BUCKET_SLASH, RestrictedStarterClaimGrantStatus,
};

pub(crate) fn add_main_token_treasury_balance(
    balances: &mut BTreeMap<String, u64>,
    bucket_id: &str,
    amount: u64,
) -> Result<(), WorldError> {
    let next = balances
        .get(bucket_id)
        .copied()
        .unwrap_or(0)
        .checked_add(amount)
        .ok_or_else(|| WorldError::ResourceBalanceInvalid {
            reason: format!(
                "main token treasury balance overflow: bucket={} amount={}",
                bucket_id, amount
            ),
        })?;
    balances.insert(bucket_id.to_string(), next);
    Ok(())
}

pub(crate) fn debit_main_token_treasury_balance(
    balances: &mut BTreeMap<String, u64>,
    bucket_id: &str,
    amount: u64,
) -> Result<(), WorldError> {
    let current = balances.get(bucket_id).copied().unwrap_or(0);
    if current < amount {
        return Err(WorldError::ResourceBalanceInvalid {
            reason: format!(
                "main token treasury insufficient: bucket={} balance={} amount={}",
                bucket_id, current, amount
            ),
        });
    }
    balances.insert(bucket_id.to_string(), current - amount);
    Ok(())
}

pub(crate) fn debit_main_token_restricted_starter_claim_balance(
    balances: &mut BTreeMap<String, MainTokenAccountBalance>,
    account_id: &str,
    amount: u64,
) -> Result<(), WorldError> {
    let Some(account) = balances.get_mut(account_id) else {
        if amount == 0 {
            return Ok(());
        }
        return Err(WorldError::ResourceBalanceInvalid {
            reason: format!("restricted grant account not found: {account_id}"),
        });
    };
    if account.restricted_starter_claim_balance < amount {
        return Err(WorldError::ResourceBalanceInvalid {
            reason: format!(
                "restricted grant balance insufficient: account={} balance={} amount={}",
                account_id, account.restricted_starter_claim_balance, amount
            ),
        });
    }
    account.restricted_starter_claim_balance -= amount;
    Ok(())
}

pub(crate) fn restricted_starter_claim_grant_can_be_inserted(
    state: &WorldState,
    beneficiary_account_id: &str,
) -> bool {
    let Some(grant) = state
        .restricted_starter_claim_grants
        .get(beneficiary_account_id)
    else {
        return true;
    };
    if grant.status == RestrictedStarterClaimGrantStatus::Issued {
        return false;
    }
    let restricted_balance = state
        .main_token_balances
        .get(beneficiary_account_id)
        .map(|balance| balance.restricted_starter_claim_balance)
        .unwrap_or(0);
    let locked_restricted = state
        .agent_claims
        .values()
        .filter(|claim| claim.claim_owner_id == beneficiary_account_id)
        .fold(0_u64, |acc, claim| {
            acc.saturating_add(claim.claim_bond_locked_restricted_amount)
        });
    restricted_balance == 0 && locked_restricted == 0
}

pub(crate) fn main_token_fee_treasury_bucket(fee_kind: MainTokenFeeKind) -> &'static str {
    match fee_kind {
        MainTokenFeeKind::GasBaseFee => MAIN_TOKEN_TREASURY_BUCKET_GAS_FEE,
        MainTokenFeeKind::SlashPenalty => MAIN_TOKEN_TREASURY_BUCKET_SLASH,
        MainTokenFeeKind::ModuleFee => MAIN_TOKEN_TREASURY_BUCKET_MODULE_FEE,
    }
}

pub(crate) fn resolve_main_token_effective_config_for_epoch(
    state: &WorldState,
    epoch_index: u64,
) -> &MainTokenConfig {
    state
        .main_token_scheduled_policy_updates
        .range(..=epoch_index)
        .next_back()
        .map(|(_, item)| &item.next_config)
        .unwrap_or(&state.main_token_config)
}
