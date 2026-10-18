//! Event-bound projection for raw main-token monetary events.

use serde::ser::SerializeStruct;
use std::collections::BTreeMap;

use super::super::{
    DomainEvent, MainTokenAccountBalance, MainTokenEpochIssuanceRecord,
    MainTokenGenesisAllocationBucketState, MainTokenSupplyState, MaterialLedgerId, WorldError,
    WorldState, WorldTime,
};
use super::economy_data_publication::SparseMapProjection;

#[derive(Debug)]
pub(crate) struct PreparedMainTokenMonetaryEvent {
    event: DomainEvent,
    supply: Option<MainTokenSupplyState>,
    balances: BTreeMap<String, MainTokenAccountBalance>,
    replace_balances: bool,
    genesis_buckets: BTreeMap<String, MainTokenGenesisAllocationBucketState>,
    replace_genesis_buckets: bool,
    claim_nonces: BTreeMap<String, u64>,
    transfer_nonces: BTreeMap<String, u64>,
    issuance_records: BTreeMap<u64, MainTokenEpochIssuanceRecord>,
    treasury_balances: BTreeMap<String, u64>,
    world_materials: BTreeMap<String, i64>,
}

impl PreparedMainTokenMonetaryEvent {
    pub(crate) fn prepare(
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<Self, WorldError> {
        let mut out = Self {
            event: event.clone(),
            supply: None,
            balances: BTreeMap::new(),
            replace_balances: false,
            genesis_buckets: BTreeMap::new(),
            replace_genesis_buckets: false,
            claim_nonces: BTreeMap::new(),
            transfer_nonces: BTreeMap::new(),
            issuance_records: BTreeMap::new(),
            treasury_balances: BTreeMap::new(),
            world_materials: state
                .material_ledgers
                .get(&MaterialLedgerId::world())
                .filter(|ledger| !ledger.is_empty())
                .unwrap_or(&state.materials)
                .clone(),
        };
        match event {
            DomainEvent::MainTokenGenesisInitialized {
                total_supply,
                allocations,
            } => out.prepare_genesis(state, *total_supply, allocations)?,
            DomainEvent::MainTokenVestingClaimed {
                bucket_id,
                beneficiary,
                amount,
                nonce,
            } => out.prepare_vesting(state, bucket_id, beneficiary, *amount, *nonce, now)?,
            DomainEvent::MainTokenTransferred {
                from_account_id,
                to_account_id,
                amount,
                nonce,
                ..
            } => out.prepare_transfer(state, from_account_id, to_account_id, *amount, *nonce)?,
            DomainEvent::MainTokenEpochIssued {
                epoch_index,
                inflation_rate_bps,
                issued_amount,
                staking_reward_amount,
                node_service_reward_amount,
                ecosystem_pool_amount,
                security_reserve_amount,
            } => out.prepare_epoch(
                state,
                *epoch_index,
                *inflation_rate_bps,
                *issued_amount,
                *staking_reward_amount,
                *node_service_reward_amount,
                *ecosystem_pool_amount,
                *security_reserve_amount,
            )?,
            DomainEvent::MainTokenFeeSettled {
                fee_kind,
                amount,
                burn_amount,
                treasury_amount,
            } => out.prepare_fee(state, *fee_kind, *amount, *burn_amount, *treasury_amount)?,
            _ => {
                return Err(invalid(
                    "main-token monetary preparation requires a monetary event",
                ));
            }
        }
        Ok(out)
    }

    fn prepare_genesis(
        &mut self,
        state: &WorldState,
        total_supply: u64,
        allocations: &[MainTokenGenesisAllocationBucketState],
    ) -> Result<(), WorldError> {
        if total_supply == 0 {
            return Err(invalid("main token genesis total_supply must be > 0"));
        }
        if allocations.is_empty() {
            return Err(invalid("main token genesis allocations cannot be empty"));
        }
        if !state.main_token_genesis_buckets.is_empty() {
            return Err(invalid("main token genesis already initialized"));
        }
        if !state.main_token_balances.is_empty()
            || !state.main_token_treasury_balances.is_empty()
            || !state.main_token_claim_nonces.is_empty()
            || !state.main_token_transfer_nonces.is_empty()
        {
            return Err(invalid(
                "main token ledger is not empty during genesis initialization",
            ));
        }
        let current = &state.main_token_supply;
        if current.total_supply > 0
            || current.total_issued > 0
            || current.total_burned > 0
            || current.circulating_supply > 0
        {
            return Err(invalid("main token supply already initialized"));
        }
        let mut ratio_sum = 0_u64;
        let mut allocated_sum = 0_u64;
        let mut recipient_vested = BTreeMap::<String, u64>::new();
        for allocation in allocations {
            if allocation.bucket_id.trim().is_empty() {
                return Err(invalid("main token allocation bucket_id cannot be empty"));
            }
            if allocation.recipient.trim().is_empty() {
                return Err(invalid(format!(
                    "main token allocation recipient cannot be empty: bucket={}",
                    allocation.bucket_id
                )));
            }
            if allocation.ratio_bps == 0 {
                return Err(invalid(format!(
                    "main token allocation ratio must be > 0: bucket={}",
                    allocation.bucket_id
                )));
            }
            if allocation.claimed_amount != 0 {
                return Err(invalid(format!(
                    "main token allocation claimed_amount must be 0 at genesis: bucket={}",
                    allocation.bucket_id
                )));
            }
            ratio_sum = ratio_sum.saturating_add(u64::from(allocation.ratio_bps));
            allocated_sum = allocated_sum.saturating_add(allocation.allocated_amount);
            if self
                .genesis_buckets
                .insert(allocation.bucket_id.clone(), allocation.clone())
                .is_some()
            {
                return Err(invalid(format!(
                    "duplicate main token allocation bucket_id: {}",
                    allocation.bucket_id
                )));
            }
            recipient_vested
                .entry(allocation.recipient.clone())
                .and_modify(|value| *value = value.saturating_add(allocation.allocated_amount))
                .or_insert(allocation.allocated_amount);
        }
        if ratio_sum != 10_000 {
            return Err(invalid(format!(
                "main token allocation ratio sum must be 10000 bps, got {ratio_sum}"
            )));
        }
        if allocated_sum != total_supply {
            return Err(invalid(format!(
                "main token allocation sum mismatch: allocated={allocated_sum} total_supply={total_supply}"
            )));
        }
        self.supply = Some(MainTokenSupplyState {
            total_supply,
            circulating_supply: 0,
            total_issued: 0,
            total_burned: 0,
        });
        self.balances = recipient_vested
            .into_iter()
            .map(|(recipient, vested_balance)| {
                (
                    recipient.clone(),
                    MainTokenAccountBalance {
                        account_id: recipient,
                        liquid_balance: 0,
                        vested_balance,
                        restricted_starter_claim_balance: 0,
                    },
                )
            })
            .collect();
        self.replace_balances = true;
        self.replace_genesis_buckets = true;
        Ok(())
    }

    fn prepare_vesting(
        &mut self,
        state: &WorldState,
        bucket_id: &str,
        beneficiary: &str,
        amount: u64,
        nonce: u64,
        now: WorldTime,
    ) -> Result<(), WorldError> {
        if amount == 0 {
            return Err(invalid("main token vesting claim amount must be > 0"));
        }
        if nonce == 0 {
            return Err(invalid("main token vesting claim nonce must be > 0"));
        }
        if let Some(last) = state.main_token_claim_nonces.get(beneficiary)
            && nonce <= *last
        {
            return Err(invalid(format!(
                "main token vesting claim nonce replay: beneficiary={beneficiary} nonce={nonce} last_nonce={last}"
            )));
        }
        let mut bucket = state
            .main_token_genesis_buckets
            .get(bucket_id)
            .cloned()
            .ok_or_else(|| invalid(format!("main token genesis bucket not found: {bucket_id}")))?;
        if bucket.recipient != beneficiary {
            return Err(invalid(format!(
                "main token vesting beneficiary mismatch: bucket recipient={} beneficiary={beneficiary}",
                bucket.recipient
            )));
        }
        let releasable = super::super::main_token::main_token_bucket_unlocked_amount(&bucket, now)
            .saturating_sub(bucket.claimed_amount);
        if releasable == 0 {
            return Err(invalid(format!(
                "main token vesting has no releasable amount: bucket={bucket_id} epoch={now}"
            )));
        }
        if amount != releasable {
            return Err(invalid(format!(
                "main token vesting claim amount mismatch: expected={releasable} actual={amount}"
            )));
        }
        let mut account = state
            .main_token_balances
            .get(beneficiary)
            .cloned()
            .ok_or_else(|| {
                invalid(format!(
                    "main token beneficiary account not found: {beneficiary}"
                ))
            })?;
        if account.vested_balance < amount {
            return Err(invalid(format!(
                "main token vested balance insufficient: beneficiary={beneficiary} vested={} claim={amount}",
                account.vested_balance
            )));
        }
        account.vested_balance -= amount;
        account.liquid_balance = account.liquid_balance.checked_add(amount).ok_or_else(|| invalid(format!("main token liquid balance overflow: beneficiary={beneficiary} current={} claim={amount}", account.liquid_balance)))?;
        bucket.claimed_amount = bucket.claimed_amount.checked_add(amount).ok_or_else(|| {
            invalid(format!(
                "main token claimed amount overflow: bucket={bucket_id} current={} claim={amount}",
                bucket.claimed_amount
            ))
        })?;
        if bucket.claimed_amount > bucket.allocated_amount {
            return Err(invalid(format!(
                "main token claimed exceeds allocation: bucket={bucket_id} claimed={} allocated={}",
                bucket.claimed_amount, bucket.allocated_amount
            )));
        }
        let mut supply = state.main_token_supply.clone();
        supply.circulating_supply =
            supply
                .circulating_supply
                .checked_add(amount)
                .ok_or_else(|| {
                    invalid(format!(
                        "main token circulating supply overflow: current={} delta={amount}",
                        supply.circulating_supply
                    ))
                })?;
        if supply.circulating_supply > supply.total_supply {
            return Err(invalid(format!(
                "main token circulating exceeds total supply: circulating={} total={}",
                supply.circulating_supply, supply.total_supply
            )));
        }
        self.balances.insert(beneficiary.to_string(), account);
        self.genesis_buckets.insert(bucket_id.to_string(), bucket);
        self.claim_nonces.insert(beneficiary.to_string(), nonce);
        self.supply = Some(supply);
        Ok(())
    }

    fn prepare_transfer(
        &mut self,
        state: &WorldState,
        from: &str,
        to: &str,
        amount: u64,
        nonce: u64,
    ) -> Result<(), WorldError> {
        let from = from.trim();
        let to = to.trim();
        if from.is_empty() {
            return Err(invalid(
                "main token transfer from_account_id cannot be empty",
            ));
        }
        if to.is_empty() {
            return Err(invalid("main token transfer to_account_id cannot be empty"));
        }
        if from == to {
            return Err(invalid(format!(
                "main token transfer from_account_id and to_account_id cannot be the same: {from}"
            )));
        }
        if amount == 0 {
            return Err(invalid("main token transfer amount must be > 0"));
        }
        if nonce == 0 {
            return Err(invalid("main token transfer nonce must be > 0"));
        }
        if let Some(last) = state.main_token_transfer_nonces.get(from)
            && nonce <= *last
        {
            return Err(invalid(format!(
                "main token transfer nonce replay: from_account_id={from} nonce={nonce} last_nonce={last}"
            )));
        }
        let mut source = state
            .main_token_balances
            .get(from)
            .cloned()
            .ok_or_else(|| {
                invalid(format!(
                    "main token transfer source account not found: {from}"
                ))
            })?;
        if source.account_id != from {
            return Err(invalid(format!(
                "main token transfer source account key mismatch: key={from} value={}",
                source.account_id
            )));
        }
        if source.liquid_balance < amount {
            return Err(invalid(format!(
                "main token transfer source balance insufficient: from_account_id={from} balance={} amount={amount}",
                source.liquid_balance
            )));
        }
        let target_before = state.main_token_balances.get(to).cloned();
        if let Some(target) = &target_before
            && target.account_id != to
        {
            return Err(invalid(format!(
                "main token transfer target account key mismatch: key={to} value={}",
                target.account_id
            )));
        }
        let target_liquid = target_before.as_ref().map(|a| a.liquid_balance).unwrap_or(0).checked_add(amount).ok_or_else(|| invalid(format!("main token transfer target balance overflow: to_account_id={to} amount={amount}")))?;
        source.liquid_balance -= amount;
        let mut target = target_before.unwrap_or_else(|| MainTokenAccountBalance {
            account_id: to.to_string(),
            ..MainTokenAccountBalance::default()
        });
        target.liquid_balance = target_liquid;
        self.balances.insert(from.to_string(), source);
        self.balances.insert(to.to_string(), target);
        self.transfer_nonces.insert(from.to_string(), nonce);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_epoch(
        &mut self,
        state: &WorldState,
        epoch: u64,
        rate: u32,
        issued: u64,
        staking: u64,
        node: u64,
        ecosystem: u64,
        security: u64,
    ) -> Result<(), WorldError> {
        if state.main_token_epoch_issuance_records.contains_key(&epoch) {
            return Err(invalid(format!(
                "main token epoch issuance already exists: epoch={epoch}"
            )));
        }
        let split = staking.checked_add(node).and_then(|v| v.checked_add(ecosystem)).and_then(|v| v.checked_add(security)).ok_or_else(|| invalid(format!("main token epoch split overflow: epoch={epoch} staking={staking} node_service={node} ecosystem={ecosystem} security={security}")))?;
        if split != issued {
            return Err(invalid(format!(
                "main token epoch split mismatch: epoch={epoch} issued={issued} split_sum={split}"
            )));
        }
        let mut supply = state.main_token_supply.clone();
        supply.total_issued = supply.total_issued.checked_add(issued).ok_or_else(|| {
            invalid(format!(
                "main token total_issued overflow: current={} issued={issued}",
                supply.total_issued
            ))
        })?;
        supply.total_supply = supply.total_supply.checked_add(issued).ok_or_else(|| {
            invalid(format!(
                "main token total_supply overflow: current={} issued={issued}",
                supply.total_supply
            ))
        })?;
        if let Some(max) = super::super::state::apply_domain_event_main_token::helpers::resolve_main_token_effective_config_for_epoch(state, epoch).max_supply && supply.total_supply > max { return Err(invalid(format!("main token total_supply exceeds max_supply: next={} max={max}", supply.total_supply))); }
        self.treasury_balances = state.main_token_treasury_balances.clone();
        for (bucket, amount) in [
            (
                super::super::MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD,
                staking,
            ),
            (
                super::super::MAIN_TOKEN_TREASURY_BUCKET_NODE_SERVICE_REWARD,
                node,
            ),
            (
                super::super::MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL,
                ecosystem,
            ),
            (
                super::super::MAIN_TOKEN_TREASURY_BUCKET_SECURITY_RESERVE,
                security,
            ),
        ] {
            super::super::state::apply_domain_event_main_token::helpers::add_main_token_treasury_balance(&mut self.treasury_balances, bucket, amount)?;
        }
        self.supply = Some(supply);
        self.issuance_records.insert(
            epoch,
            MainTokenEpochIssuanceRecord {
                epoch_index: epoch,
                inflation_rate_bps: rate,
                issued_amount: issued,
                staking_reward_amount: staking,
                node_service_reward_amount: node,
                ecosystem_pool_amount: ecosystem,
                security_reserve_amount: security,
            },
        );
        Ok(())
    }

    fn prepare_fee(
        &mut self,
        state: &WorldState,
        kind: super::super::MainTokenFeeKind,
        amount: u64,
        burn: u64,
        treasury: u64,
    ) -> Result<(), WorldError> {
        if amount == 0 {
            return Err(invalid("main token fee amount must be > 0"));
        }
        let sum = burn.checked_add(treasury).ok_or_else(|| {
            invalid(format!(
                "main token fee settled overflow: amount={amount} burn={burn} treasury={treasury}"
            ))
        })?;
        if sum != amount {
            return Err(invalid(format!(
                "main token fee settled mismatch: amount={amount} burn={burn} treasury={treasury}"
            )));
        }
        let mut supply = state.main_token_supply.clone();
        if supply.circulating_supply < amount {
            return Err(invalid(format!(
                "main token circulating supply insufficient for fee settlement: circulating={} amount={amount}",
                supply.circulating_supply
            )));
        }
        if supply.total_supply < burn {
            return Err(invalid(format!(
                "main token total_supply insufficient for burn: total={} burn={burn}",
                supply.total_supply
            )));
        }
        supply.circulating_supply -= amount;
        supply.total_supply -= burn;
        supply.total_burned = supply.total_burned.checked_add(burn).ok_or_else(|| {
            invalid(format!(
                "main token total_burned overflow: current={} burn={burn}",
                supply.total_burned
            ))
        })?;
        self.treasury_balances = state.main_token_treasury_balances.clone();
        let bucket = super::super::state::apply_domain_event_main_token::helpers::main_token_fee_treasury_bucket(kind);
        super::super::state::apply_domain_event_main_token::helpers::add_main_token_treasury_balance(&mut self.treasury_balances, bucket, treasury)?;
        if supply.circulating_supply > supply.total_supply {
            return Err(invalid(format!(
                "main token circulating exceeds total supply after fee settlement: circulating={} total={}",
                supply.circulating_supply, supply.total_supply
            )));
        }
        self.supply = Some(supply);
        Ok(())
    }

    pub(crate) fn matches_event(&self, event: &DomainEvent) -> bool {
        &self.event == event
    }
    fn routed_agents(&self, state: &WorldState) -> BTreeMap<String, super::super::AgentCell> {
        let id = match &self.event {
            DomainEvent::MainTokenVestingClaimed { beneficiary, .. } => Some(beneficiary),
            DomainEvent::MainTokenTransferred {
                from_account_id, ..
            } => Some(from_account_id),
            _ => None,
        };
        let mut out = BTreeMap::new();
        if let Some(id) = id
            && let Some(mut cell) = state.agents.get(id).cloned()
        {
            cell.mailbox.push_back(self.event.clone());
            out.insert(id.clone(), cell);
        }
        out
    }
    pub(crate) fn install_infallible(self, state: &mut WorldState) {
        if let Some(supply) = self.supply {
            state.main_token_supply = supply;
        }
        if self.replace_balances {
            state.main_token_balances = self.balances;
        } else {
            state.main_token_balances.extend(self.balances);
        }
        if self.replace_genesis_buckets {
            state.main_token_genesis_buckets = self.genesis_buckets;
        } else {
            state
                .main_token_genesis_buckets
                .extend(self.genesis_buckets);
        }
        state.main_token_claim_nonces.extend(self.claim_nonces);
        state
            .main_token_transfer_nonces
            .extend(self.transfer_nonces);
        state
            .main_token_epoch_issuance_records
            .extend(self.issuance_records);
        if !self.treasury_balances.is_empty() {
            state.main_token_treasury_balances = self.treasury_balances;
        }
        state.materials = self.world_materials.clone();
        state
            .material_ledgers
            .insert(MaterialLedgerId::world(), self.world_materials);
    }
    pub(crate) fn serialize_agents<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        let updates = self.routed_agents(state);
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
        if self.replace_balances {
            out.serialize_field("main_token_balances", &self.balances)
        } else {
            out.serialize_field(
                "main_token_balances",
                &SparseMapProjection {
                    base: &state.main_token_balances,
                    updates: &self.balances,
                    deletion: None,
                },
            )
        }
    }
    pub(crate) fn serialize_buckets<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        if self.replace_genesis_buckets {
            out.serialize_field("main_token_genesis_buckets", &self.genesis_buckets)
        } else {
            out.serialize_field(
                "main_token_genesis_buckets",
                &SparseMapProjection {
                    base: &state.main_token_genesis_buckets,
                    updates: &self.genesis_buckets,
                    deletion: None,
                },
            )
        }
    }
    pub(crate) fn serialize_issuance<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field(
            "main_token_epoch_issuance_records",
            &SparseMapProjection {
                base: &state.main_token_epoch_issuance_records,
                updates: &self.issuance_records,
                deletion: None,
            },
        )
    }
    pub(crate) fn serialize_treasury<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        if self.treasury_balances.is_empty() {
            out.serialize_field(
                "main_token_treasury_balances",
                &state.main_token_treasury_balances,
            )
        } else {
            out.serialize_field("main_token_treasury_balances", &self.treasury_balances)
        }
    }
    pub(crate) fn serialize_claim_nonces<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field(
            "main_token_claim_nonces",
            &SparseMapProjection {
                base: &state.main_token_claim_nonces,
                updates: &self.claim_nonces,
                deletion: None,
            },
        )
    }
    pub(crate) fn serialize_transfer_nonces<S: SerializeStruct>(
        &self,
        state: &WorldState,
        out: &mut S,
    ) -> Result<(), S::Error> {
        out.serialize_field(
            "main_token_transfer_nonces",
            &SparseMapProjection {
                base: &state.main_token_transfer_nonces,
                updates: &self.transfer_nonces,
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
