//! Event-bound projection for raw main-token governance monetary events.

use serde::ser::SerializeStruct;
use std::collections::{BTreeMap, BTreeSet};

use super::super::{
    DomainEvent, MainTokenAccountBalance, MainTokenConfig, MainTokenScheduledPolicyUpdate,
    MainTokenSupplyState, MainTokenTreasuryDistribution, MainTokenTreasuryDistributionRecord,
    MaterialLedgerId, ProposalId, WorldError, WorldState, WorldTime,
};
use super::economy_data_publication::SparseMapProjection;

#[derive(Debug)]
pub(crate) struct PreparedMainTokenGovernanceMonetaryEvent {
    event: DomainEvent,
    scheduled_updates: BTreeMap<u64, MainTokenScheduledPolicyUpdate>,
    treasury_balances: BTreeMap<String, u64>,
    balances: BTreeMap<String, MainTokenAccountBalance>,
    supply: Option<MainTokenSupplyState>,
    distribution_records: BTreeMap<String, MainTokenTreasuryDistributionRecord>,
    world_materials: BTreeMap<String, i64>,
}

impl PreparedMainTokenGovernanceMonetaryEvent {
    pub(crate) fn prepare(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        let mut prepared = Self {
            event: event.clone(),
            scheduled_updates: BTreeMap::new(),
            treasury_balances: BTreeMap::new(),
            balances: BTreeMap::new(),
            supply: None,
            distribution_records: BTreeMap::new(),
            world_materials: state
                .material_ledgers
                .get(&MaterialLedgerId::world())
                .filter(|ledger| !ledger.is_empty())
                .unwrap_or(&state.materials)
                .clone(),
        };
        match event {
            DomainEvent::MainTokenPolicyUpdateScheduled {
                proposal_id,
                effective_epoch,
                next,
            } => prepared.prepare_policy(state, *proposal_id, *effective_epoch, next, now)?,
            DomainEvent::MainTokenTreasuryDistributed {
                proposal_id,
                distribution_id,
                bucket_id,
                total_amount,
                distributions,
            } => prepared.prepare_distribution(
                state,
                *proposal_id,
                distribution_id,
                bucket_id,
                *total_amount,
                distributions,
                now,
            )?,
            _ => {
                return Err(invalid(
                    "main-token governance monetary preparation requires a supported event",
                ));
            }
        }
        Ok(prepared)
    }

    fn prepare_policy(
        &mut self,
        state: &WorldState,
        proposal_id: ProposalId,
        effective_epoch: u64,
        next: &MainTokenConfig,
        now: WorldTime,
    ) -> Result<(), WorldError> {
        if proposal_id == 0 {
            return Err(invalid("main token policy proposal_id must be > 0"));
        }
        if effective_epoch <= now {
            return Err(invalid(format!(
                "main token policy effective_epoch must be > now: effective={effective_epoch} now={now}"
            )));
        }
        if let Err(reason) = super::super::main_token::validate_main_token_config_bounds(next) {
            return Err(invalid(format!(
                "main token policy config out of bounds: {reason}"
            )));
        }
        if next.initial_supply != state.main_token_config.initial_supply {
            return Err(invalid(format!(
                "main token policy cannot change initial_supply: current={} next={}",
                state.main_token_config.initial_supply, next.initial_supply
            )));
        }
        if let Some(max) = next.max_supply
            && max < state.main_token_supply.total_supply
        {
            return Err(invalid(format!(
                "main token policy max_supply cannot be below current total_supply: max={max} total={}",
                state.main_token_supply.total_supply
            )));
        }
        if state
            .main_token_scheduled_policy_updates
            .contains_key(&effective_epoch)
        {
            return Err(invalid(format!(
                "main token policy effective_epoch already scheduled: {effective_epoch}"
            )));
        }
        if state
            .main_token_scheduled_policy_updates
            .values()
            .any(|item| item.proposal_id == proposal_id)
        {
            return Err(invalid(format!(
                "main token policy proposal already scheduled: {proposal_id}"
            )));
        }
        self.scheduled_updates.insert(
            effective_epoch,
            MainTokenScheduledPolicyUpdate {
                proposal_id,
                effective_epoch,
                next_config: next.clone(),
            },
        );
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_distribution(
        &mut self,
        state: &WorldState,
        proposal_id: ProposalId,
        distribution_id: &str,
        bucket_id: &str,
        total_amount: u64,
        distributions: &[MainTokenTreasuryDistribution],
        now: WorldTime,
    ) -> Result<(), WorldError> {
        if proposal_id == 0 {
            return Err(invalid(
                "main token treasury distribution proposal_id must be > 0",
            ));
        }
        let distribution_id = distribution_id.trim();
        if distribution_id.is_empty() {
            return Err(invalid(
                "main token treasury distribution_id cannot be empty",
            ));
        }
        let bucket_id = bucket_id.trim();
        if !super::super::main_token::is_main_token_treasury_distribution_bucket(bucket_id) {
            return Err(invalid(format!(
                "main token treasury distribution bucket is not allowed: {bucket_id}"
            )));
        }
        if total_amount == 0 {
            return Err(invalid("main token treasury total_amount must be > 0"));
        }
        if distributions.is_empty() {
            return Err(invalid(
                "main token treasury distribution list cannot be empty",
            ));
        }
        if state
            .main_token_treasury_distribution_records
            .contains_key(distribution_id)
        {
            return Err(invalid(format!(
                "main token treasury distribution_id already exists: {distribution_id}"
            )));
        }
        let mut seen = BTreeSet::new();
        let mut sum = 0_u64;
        for item in distributions {
            let account_id = item.account_id.trim();
            if account_id.is_empty() {
                return Err(invalid(format!(
                    "main token treasury distribution account_id cannot be empty: distribution_id={distribution_id}"
                )));
            }
            if item.amount == 0 {
                return Err(invalid(format!(
                    "main token treasury distribution amount must be > 0: distribution_id={distribution_id} account_id={account_id}"
                )));
            }
            if !seen.insert(account_id.to_string()) {
                return Err(invalid(format!(
                    "duplicate main token treasury distribution account_id: distribution_id={distribution_id} account_id={account_id}"
                )));
            }
            sum = sum.checked_add(item.amount).ok_or_else(|| {
                invalid(format!(
                    "main token treasury distribution sum overflow: distribution_id={distribution_id}"
                ))
            })?;
        }
        if sum != total_amount {
            return Err(invalid(format!(
                "main token treasury distribution sum mismatch: distribution_id={distribution_id} total={total_amount} sum={sum}"
            )));
        }
        let bucket_balance = state
            .main_token_treasury_balances
            .get(bucket_id)
            .copied()
            .unwrap_or(0);
        if bucket_balance < total_amount {
            return Err(invalid(format!(
                "main token treasury bucket insufficient: bucket={bucket_id} balance={bucket_balance} total={total_amount}"
            )));
        }
        self.treasury_balances
            .insert(bucket_id.to_string(), bucket_balance - total_amount);
        for item in distributions {
            let account_id = item.account_id.trim();
            let mut account = self
                .balances
                .get(account_id)
                .cloned()
                .or_else(|| state.main_token_balances.get(account_id).cloned())
                .unwrap_or_else(|| MainTokenAccountBalance {
                    account_id: account_id.to_string(),
                    ..MainTokenAccountBalance::default()
                });
            if account.account_id != account_id {
                return Err(invalid(format!(
                    "main token treasury account key mismatch: key={account_id} value={}",
                    account.account_id
                )));
            }
            account.liquid_balance = account.liquid_balance.checked_add(item.amount).ok_or_else(|| {
                invalid(format!(
                    "main token treasury account overflow: account={account_id} current={} amount={}",
                    account.liquid_balance, item.amount
                ))
            })?;
            self.balances.insert(account_id.to_string(), account);
        }
        let mut supply = state.main_token_supply.clone();
        supply.circulating_supply = supply
            .circulating_supply
            .checked_add(total_amount)
            .ok_or_else(|| {
                invalid(format!(
                    "main token circulating overflow: current={} amount={total_amount}",
                    supply.circulating_supply
                ))
            })?;
        if supply.circulating_supply > supply.total_supply {
            return Err(invalid(format!(
                "main token circulating exceeds total: circulating={} total={}",
                supply.circulating_supply, supply.total_supply
            )));
        }
        self.supply = Some(supply);
        self.distribution_records.insert(
            distribution_id.to_string(),
            MainTokenTreasuryDistributionRecord {
                proposal_id,
                distribution_id: distribution_id.to_string(),
                bucket_id: bucket_id.to_string(),
                total_amount,
                distributions: distributions.to_vec(),
                distributed_epoch: now,
            },
        );
        Ok(())
    }

    pub(crate) fn matches_event(&self, event: &DomainEvent) -> bool {
        &self.event == event
    }

    pub(crate) fn install_infallible(self, state: &mut WorldState) {
        state
            .main_token_scheduled_policy_updates
            .extend(self.scheduled_updates);
        state
            .main_token_treasury_balances
            .extend(self.treasury_balances);
        state.main_token_balances.extend(self.balances);
        if let Some(supply) = self.supply {
            state.main_token_supply = supply;
        }
        state
            .main_token_treasury_distribution_records
            .extend(self.distribution_records);
        state.materials = self.world_materials.clone();
        state
            .material_ledgers
            .insert(MaterialLedgerId::world(), self.world_materials);
    }

    pub(crate) fn serialize_materials<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field("materials", &self.world_materials)?;
        let updates = BTreeMap::from([(MaterialLedgerId::world(), self.world_materials.clone())]);
        out.serialize_field(
            "material_ledgers",
            &SparseMapProjection {
                base: &state.material_ledgers,
                updates: &updates,
                deletion: None,
            },
        )
    }
    pub(crate) fn serialize_supply_balances<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field(
            "main_token_supply",
            self.supply.as_ref().unwrap_or(&state.main_token_supply),
        )?;
        out.serialize_field(
            "main_token_balances",
            &SparseMapProjection {
                base: &state.main_token_balances,
                updates: &self.balances,
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
                updates: &self.treasury_balances,
                deletion: None,
            },
        )
    }
    pub(crate) fn serialize_scheduled<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field(
            "main_token_scheduled_policy_updates",
            &SparseMapProjection {
                base: &state.main_token_scheduled_policy_updates,
                updates: &self.scheduled_updates,
                deletion: None,
            },
        )
    }
    pub(crate) fn serialize_distributions<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field(
            "main_token_treasury_distribution_records",
            &SparseMapProjection {
                base: &state.main_token_treasury_distribution_records,
                updates: &self.distribution_records,
                deletion: None,
            },
        )
    }
}

fn invalid(reason: impl Into<String>) -> WorldError {
    WorldError::ResourceBalanceInvalid {
        reason: reason.into(),
    }
}
