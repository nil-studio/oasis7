use super::super::ResourceDelta;
use super::super::WorldError;
use super::super::main_token::main_token_account_id_from_node_public_key;
use super::super::reward_asset::{
    REWARD_MINT_SIGNATURE_V1_PREFIX, REWARD_MINT_SIGNATURE_V2_PREFIX, reward_mint_signature_v1,
    reward_mint_signature_v2, verify_reward_mint_signature_v2, verify_reward_redeem_signature_v1,
};
use super::super::util::hash_json;
use super::super::{
    EpochSettlementReport, FactoryProfileV1, MainTokenAccountBalance, MainTokenConfig,
    MainTokenEpochIssuanceRecord, MainTokenGenesisAllocationBucketState,
    MainTokenNodePointsBridgeEpochRecord, MainTokenScheduledPolicyUpdate, MainTokenSupplyState,
    MainTokenTreasuryDistributionRecord, MaterialLedgerId, MaterialProfileV1, MaterialStack,
    NodeAssetBalance, NodeRewardMintRecord, ProductProfileV1, ProtocolPowerReserve,
    RecipeProfileV1, RestrictedStarterClaimGrantState,
    RestrictedStarterClaimLiveopsPoolTopUpRecord, RewardAssetConfig, RewardAssetInvariantReport,
    RewardAssetInvariantViolation, RewardSignatureGovernancePolicy, SystemOrderPoolBudget,
};
use super::World;
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_agent_api::world_resource::StockError;
use std::collections::BTreeMap;

impl World {
    // ---------------------------------------------------------------------
    // Main token ledger
    // ---------------------------------------------------------------------

    pub fn main_token_config(&self) -> &MainTokenConfig {
        &self.state.main_token_config
    }

    pub fn set_main_token_config(&mut self, config: MainTokenConfig) {
        self.state.main_token_config = config;
    }

    pub fn main_token_supply(&self) -> &MainTokenSupplyState {
        &self.state.main_token_supply
    }

    pub fn set_main_token_supply(&mut self, supply: MainTokenSupplyState) {
        self.state.main_token_supply = supply;
    }

    pub fn main_token_account_balance(&self, account_id: &str) -> Option<&MainTokenAccountBalance> {
        self.state.main_token_balances.get(account_id)
    }

    pub fn main_token_liquid_balance(&self, account_id: &str) -> u64 {
        self.state
            .main_token_balances
            .get(account_id)
            .map(|balance| balance.liquid_balance)
            .unwrap_or(0)
    }

    pub fn main_token_restricted_starter_claim_balance(&self, account_id: &str) -> u64 {
        self.state
            .main_token_balances
            .get(account_id)
            .map(|balance| balance.restricted_starter_claim_balance)
            .unwrap_or(0)
    }

    pub fn main_token_claim_eligible_balance(&self, account_id: &str, slot_index: u8) -> u64 {
        match slot_index {
            1 => self
                .main_token_liquid_balance(account_id)
                .saturating_add(self.main_token_restricted_starter_claim_balance(account_id)),
            _ => self.main_token_liquid_balance(account_id),
        }
    }

    pub fn main_token_account_balances(&self) -> Vec<MainTokenAccountBalance> {
        self.state.main_token_balances.values().cloned().collect()
    }

    pub fn restricted_starter_claim_grant(
        &self,
        account_id: &str,
    ) -> Option<&RestrictedStarterClaimGrantState> {
        self.state.restricted_starter_claim_grants.get(account_id)
    }

    pub fn set_main_token_account_balance(
        &mut self,
        account_id: &str,
        liquid_balance: u64,
        vested_balance: u64,
    ) -> Result<(), WorldError> {
        self.set_main_token_account_balance_with_restricted(
            account_id,
            liquid_balance,
            vested_balance,
            0,
        )
    }

    pub fn set_main_token_account_balance_with_restricted(
        &mut self,
        account_id: &str,
        liquid_balance: u64,
        vested_balance: u64,
        restricted_starter_claim_balance: u64,
    ) -> Result<(), WorldError> {
        let account_id = account_id.trim();
        if account_id.is_empty() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "main token account_id cannot be empty".to_string(),
            });
        }
        self.state.main_token_balances.insert(
            account_id.to_string(),
            MainTokenAccountBalance {
                account_id: account_id.to_string(),
                liquid_balance,
                vested_balance,
                restricted_starter_claim_balance,
            },
        );
        Ok(())
    }

    pub fn main_token_genesis_bucket(
        &self,
        bucket_id: &str,
    ) -> Option<&MainTokenGenesisAllocationBucketState> {
        self.state.main_token_genesis_buckets.get(bucket_id)
    }

    pub fn set_main_token_genesis_bucket(
        &mut self,
        bucket: MainTokenGenesisAllocationBucketState,
    ) -> Result<(), WorldError> {
        if bucket.bucket_id.trim().is_empty() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "main token bucket_id cannot be empty".to_string(),
            });
        }
        self.state
            .main_token_genesis_buckets
            .insert(bucket.bucket_id.clone(), bucket);
        Ok(())
    }

    pub fn main_token_epoch_issuance_record(
        &self,
        epoch_index: u64,
    ) -> Option<&MainTokenEpochIssuanceRecord> {
        self.state
            .main_token_epoch_issuance_records
            .get(&epoch_index)
    }

    pub fn record_main_token_epoch_issuance(
        &mut self,
        record: MainTokenEpochIssuanceRecord,
    ) -> Result<(), WorldError> {
        self.state
            .main_token_epoch_issuance_records
            .insert(record.epoch_index, record);
        Ok(())
    }

    pub fn main_token_treasury_balance(&self, bucket_id: &str) -> u64 {
        self.state
            .main_token_treasury_balances
            .get(bucket_id)
            .copied()
            .unwrap_or(0)
    }

    pub fn main_token_last_claim_nonce(&self, beneficiary: &str) -> Option<u64> {
        self.state.main_token_claim_nonces.get(beneficiary).copied()
    }

    pub fn main_token_last_transfer_nonce(&self, from_account_id: &str) -> Option<u64> {
        self.state
            .main_token_transfer_nonces
            .get(from_account_id)
            .copied()
    }

    pub fn main_token_scheduled_policy_update(
        &self,
        effective_epoch: u64,
    ) -> Option<&MainTokenScheduledPolicyUpdate> {
        self.state
            .main_token_scheduled_policy_updates
            .get(&effective_epoch)
    }

    pub fn main_token_node_points_bridge_record(
        &self,
        epoch_index: u64,
    ) -> Option<&MainTokenNodePointsBridgeEpochRecord> {
        self.state
            .main_token_node_points_bridge_records
            .get(&epoch_index)
    }

    pub fn main_token_treasury_distribution_record(
        &self,
        distribution_id: &str,
    ) -> Option<&MainTokenTreasuryDistributionRecord> {
        self.state
            .main_token_treasury_distribution_records
            .get(distribution_id)
    }

    pub fn restricted_starter_claim_liveops_pool_top_up_record(
        &self,
        top_up_id: &str,
    ) -> Option<&RestrictedStarterClaimLiveopsPoolTopUpRecord> {
        self.state
            .restricted_starter_claim_liveops_pool_top_up_records
            .get(top_up_id)
    }

    pub fn set_main_token_treasury_balance(
        &mut self,
        bucket_id: &str,
        amount: u64,
    ) -> Result<(), WorldError> {
        let bucket_id = bucket_id.trim();
        if bucket_id.is_empty() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "main token treasury bucket_id cannot be empty".to_string(),
            });
        }
        self.state
            .main_token_treasury_balances
            .insert(bucket_id.to_string(), amount);
        Ok(())
    }

    // ---------------------------------------------------------------------
    // Reward asset ledger
    // ---------------------------------------------------------------------

    pub fn reward_asset_config(&self) -> &RewardAssetConfig {
        &self.state.reward_asset_config
    }

    pub fn set_reward_asset_config(&mut self, config: RewardAssetConfig) {
        self.state.reward_asset_config = config;
    }

    pub fn reward_signature_governance_policy(&self) -> &RewardSignatureGovernancePolicy {
        &self.state.reward_signature_governance_policy
    }

    pub fn set_reward_signature_governance_policy(
        &mut self,
        policy: RewardSignatureGovernancePolicy,
    ) {
        self.state.reward_signature_governance_policy = policy;
    }

    pub fn protocol_power_reserve(&self) -> &ProtocolPowerReserve {
        &self.state.protocol_power_reserve
    }

    pub fn set_protocol_power_reserve(&mut self, reserve: ProtocolPowerReserve) {
        self.state.protocol_power_reserve = reserve;
    }

    pub fn node_asset_balance(&self, node_id: &str) -> Option<&NodeAssetBalance> {
        self.state.node_asset_balances.get(node_id)
    }

    pub fn node_power_credit_balance(&self, node_id: &str) -> u64 {
        self.state
            .node_asset_balances
            .get(node_id)
            .map(|balance| balance.power_credit_balance)
            .unwrap_or(0)
    }

    pub fn node_last_redeem_nonce(&self, node_id: &str) -> Option<u64> {
        self.state.node_redeem_nonces.get(node_id).copied()
    }

    pub fn node_identity_public_key(&self, node_id: &str) -> Option<&str> {
        self.state
            .node_identity_bindings
            .get(node_id)
            .map(String::as_str)
    }

    pub fn node_main_token_account(&self, node_id: &str) -> Option<&str> {
        self.state
            .node_main_token_account_bindings
            .get(node_id)
            .map(String::as_str)
    }

    pub fn bind_node_identity(
        &mut self,
        node_id: &str,
        public_key_hex: &str,
    ) -> Result<(), WorldError> {
        let node_id = node_id.trim();
        if node_id.is_empty() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "node_id cannot be empty".to_string(),
            });
        }
        let public_key_hex = public_key_hex.trim();
        if public_key_hex.is_empty() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "public_key_hex cannot be empty".to_string(),
            });
        }
        self.state
            .node_identity_bindings
            .insert(node_id.to_string(), public_key_hex.to_string());
        self.state
            .node_main_token_account_bindings
            .entry(node_id.to_string())
            .or_insert_with(|| main_token_account_id_from_node_public_key(public_key_hex));
        Ok(())
    }

    pub fn bind_node_main_token_account(
        &mut self,
        node_id: &str,
        account_id: &str,
    ) -> Result<(), WorldError> {
        let node_id = node_id.trim();
        if node_id.is_empty() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "node_id cannot be empty".to_string(),
            });
        }
        let account_id = account_id.trim();
        if account_id.is_empty() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "main token account_id cannot be empty".to_string(),
            });
        }
        self.state
            .node_main_token_account_bindings
            .insert(node_id.to_string(), account_id.to_string());
        Ok(())
    }

    pub fn reward_mint_records(&self) -> &[NodeRewardMintRecord] {
        self.state.reward_mint_records.as_slice()
    }

    pub fn reward_asset_invariant_report(&self) -> RewardAssetInvariantReport {
        let mut report = RewardAssetInvariantReport {
            total_nodes: self.state.node_asset_balances.len(),
            mint_record_count: self.state.reward_mint_records.len(),
            ..RewardAssetInvariantReport::default()
        };

        for (node_id_key, balance) in &self.state.node_asset_balances {
            if balance.node_id.trim().is_empty() {
                report.violations.push(RewardAssetInvariantViolation {
                    code: "node_id_empty".to_string(),
                    message: "node asset balance contains empty node_id".to_string(),
                });
            } else if node_id_key != &balance.node_id {
                report.violations.push(RewardAssetInvariantViolation {
                    code: "node_id_key_mismatch".to_string(),
                    message: format!(
                        "node asset key {} mismatches payload {}",
                        node_id_key, balance.node_id
                    ),
                });
            }

            report.total_minted_credits = report
                .total_minted_credits
                .saturating_add(balance.total_minted_credits);
            report.total_burned_credits = report
                .total_burned_credits
                .saturating_add(balance.total_burned_credits);
            report.total_power_credit_balance = report
                .total_power_credit_balance
                .saturating_add(balance.power_credit_balance);

            if balance.total_burned_credits > balance.total_minted_credits {
                report.violations.push(RewardAssetInvariantViolation {
                    code: "node_burn_over_mint".to_string(),
                    message: format!(
                        "node {} has burned > minted (burned={} minted={})",
                        balance.node_id, balance.total_burned_credits, balance.total_minted_credits
                    ),
                });
                continue;
            }

            let expected_balance = balance
                .total_minted_credits
                .saturating_sub(balance.total_burned_credits);
            if balance.power_credit_balance != expected_balance {
                report.violations.push(RewardAssetInvariantViolation {
                    code: "node_balance_mismatch".to_string(),
                    message: format!(
                        "node {} balance mismatch (actual={} expected={})",
                        balance.node_id, balance.power_credit_balance, expected_balance
                    ),
                });
            }
        }

        let expected_total_balance = report
            .total_minted_credits
            .saturating_sub(report.total_burned_credits);
        if report.total_power_credit_balance != expected_total_balance {
            report.violations.push(RewardAssetInvariantViolation {
                code: "global_balance_mismatch".to_string(),
                message: format!(
                    "global power_credit balance mismatch (actual={} expected={})",
                    report.total_power_credit_balance, expected_total_balance
                ),
            });
        }

        for record in &self.state.reward_mint_records {
            if let Err(reason) = self.verify_reward_mint_record_signature(record) {
                report.violations.push(RewardAssetInvariantViolation {
                    code: "mint_signature_invalid".to_string(),
                    message: format!(
                        "mint record signature invalid (epoch={} node={}): {reason}",
                        record.epoch_index, record.node_id
                    ),
                });
            }
        }

        report
    }

    pub fn verify_reward_mint_record_signature(
        &self,
        record: &NodeRewardMintRecord,
    ) -> Result<(), String> {
        let signer_public_key = self
            .node_identity_public_key(record.signer_node_id.as_str())
            .ok_or_else(|| {
                format!(
                    "reward mint signer identity is not bound: {}",
                    record.signer_node_id
                )
            })?;
        if record
            .signature
            .starts_with(REWARD_MINT_SIGNATURE_V2_PREFIX)
        {
            return verify_reward_mint_signature_v2(
                record.signature.as_str(),
                record.epoch_index,
                record.node_id.as_str(),
                record.source_awarded_points,
                record.minted_power_credits,
                record.settlement_hash.as_str(),
                record.signer_node_id.as_str(),
                signer_public_key,
            );
        }
        if record
            .signature
            .starts_with(REWARD_MINT_SIGNATURE_V1_PREFIX)
        {
            if !self
                .state
                .reward_signature_governance_policy
                .allow_mintsig_v1_fallback
            {
                return Err("mintsig:v1 is disabled by governance policy".to_string());
            }
            let expected_signature = reward_mint_signature_v1(
                record.epoch_index,
                record.node_id.as_str(),
                record.source_awarded_points,
                record.minted_power_credits,
                record.settlement_hash.as_str(),
                record.signer_node_id.as_str(),
                signer_public_key,
            );
            if record.signature != expected_signature {
                return Err(format!(
                    "reward mint signature mismatch for node {} at epoch {}",
                    record.node_id, record.epoch_index
                ));
            }
            return Ok(());
        }
        Err(format!(
            "unsupported reward mint signature version for node {} at epoch {}",
            record.node_id, record.epoch_index
        ))
    }

    pub fn verify_redeem_power_signature(
        &self,
        node_id: &str,
        target_agent_id: &str,
        redeem_credits: u64,
        nonce: u64,
        signer_node_id: &str,
        signature: &str,
    ) -> Result<(), String> {
        let signer_public_key = self
            .node_identity_public_key(signer_node_id)
            .ok_or_else(|| format!("redeem signer identity is not bound: {signer_node_id}"))?;
        verify_reward_redeem_signature_v1(
            signature,
            node_id,
            target_agent_id,
            redeem_credits,
            nonce,
            signer_node_id,
            signer_public_key,
        )
    }

    pub fn system_order_pool_budget(&self, epoch_index: u64) -> Option<&SystemOrderPoolBudget> {
        self.state.system_order_pool_budgets.get(&epoch_index)
    }

    pub fn set_system_order_pool_budget(&mut self, epoch_index: u64, total_credit_budget: u64) {
        self.state.system_order_pool_budgets.insert(
            epoch_index,
            SystemOrderPoolBudget {
                epoch_index,
                total_credit_budget,
                remaining_credit_budget: total_credit_budget,
                node_credit_caps: BTreeMap::new(),
                node_credit_allocated: BTreeMap::new(),
            },
        );
    }

    pub fn apply_node_points_settlement_mint(
        &mut self,
        report: &EpochSettlementReport,
        signer_node_id: &str,
    ) -> Result<Vec<NodeRewardMintRecord>, WorldError> {
        if self
            .state
            .reward_signature_governance_policy
            .require_mintsig_v2
        {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "mintsig:v2 is required by governance policy; use apply_node_points_settlement_mint_v2".to_string(),
            });
        }
        self.apply_node_points_settlement_mint_internal(report, signer_node_id, None)
    }

    pub fn apply_node_points_settlement_mint_v2(
        &mut self,
        report: &EpochSettlementReport,
        signer_node_id: &str,
        signer_private_key_hex: &str,
    ) -> Result<Vec<NodeRewardMintRecord>, WorldError> {
        self.apply_node_points_settlement_mint_internal(
            report,
            signer_node_id,
            Some(signer_private_key_hex),
        )
    }

    fn apply_node_points_settlement_mint_internal(
        &mut self,
        report: &EpochSettlementReport,
        signer_node_id: &str,
        signer_private_key_hex: Option<&str>,
    ) -> Result<Vec<NodeRewardMintRecord>, WorldError> {
        if signer_node_id.trim().is_empty() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "signer_node_id cannot be empty".to_string(),
            });
        }
        let points_per_credit = self.state.reward_asset_config.points_per_credit;
        if points_per_credit == 0 {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "points_per_credit must be positive".to_string(),
            });
        }
        let signer_public_key = self
            .require_bound_node_identity(signer_node_id)?
            .to_string();
        for settlement in &report.settlements {
            self.require_bound_node_identity(settlement.node_id.as_str())?;
        }
        self.ensure_system_order_budget_caps_for_epoch(report);

        let settlement_hash = hash_json(report)?;
        let mut minted_records = Vec::new();
        for settlement in &report.settlements {
            if self.state.reward_mint_records.iter().any(|record| {
                record.epoch_index == report.epoch_index && record.node_id == settlement.node_id
            }) {
                continue;
            }

            let minted_power_credits = settlement.awarded_points / points_per_credit;
            let minted_power_credits = self.cap_minted_credits_by_system_order_budget(
                report.epoch_index,
                settlement.node_id.as_str(),
                minted_power_credits,
            );
            if minted_power_credits == 0 {
                continue;
            }
            self.mint_node_power_credits(settlement.node_id.as_str(), minted_power_credits)?;
            let signature = if let Some(signer_private_key_hex) = signer_private_key_hex {
                reward_mint_signature_v2(
                    report.epoch_index,
                    settlement.node_id.as_str(),
                    settlement.awarded_points,
                    minted_power_credits,
                    settlement_hash.as_str(),
                    signer_node_id,
                    signer_public_key.as_str(),
                    signer_private_key_hex,
                )
                .map_err(|reason| WorldError::ResourceBalanceInvalid { reason })?
            } else {
                reward_mint_signature_v1(
                    report.epoch_index,
                    settlement.node_id.as_str(),
                    settlement.awarded_points,
                    minted_power_credits,
                    settlement_hash.as_str(),
                    signer_node_id,
                    signer_public_key.as_str(),
                )
            };

            let record = NodeRewardMintRecord {
                epoch_index: report.epoch_index,
                node_id: settlement.node_id.clone(),
                source_awarded_points: settlement.awarded_points,
                minted_power_credits,
                settlement_hash: settlement_hash.clone(),
                signer_node_id: signer_node_id.to_string(),
                signature,
            };
            self.state.reward_mint_records.push(record.clone());
            minted_records.push(record);
        }

        Ok(minted_records)
    }

    fn ensure_system_order_budget_caps_for_epoch(&mut self, report: &EpochSettlementReport) {
        let Some(budget) = self
            .state
            .system_order_pool_budgets
            .get_mut(&report.epoch_index)
        else {
            return;
        };
        if !budget.node_credit_caps.is_empty() {
            return;
        }
        if budget.total_credit_budget == 0 || report.settlements.is_empty() {
            return;
        }

        let total_awarded_points = report
            .settlements
            .iter()
            .map(|settlement| settlement.awarded_points)
            .sum::<u64>();
        if total_awarded_points == 0 {
            return;
        }

        let mut distributed = 0_u64;
        for settlement in &report.settlements {
            let cap = budget
                .total_credit_budget
                .saturating_mul(settlement.awarded_points)
                / total_awarded_points;
            distributed = distributed.saturating_add(cap);
            budget
                .node_credit_caps
                .insert(settlement.node_id.clone(), cap);
        }

        let mut remainder = budget.total_credit_budget.saturating_sub(distributed);
        if remainder == 0 {
            return;
        }
        let mut ranked = report
            .settlements
            .iter()
            .map(|settlement| (settlement.node_id.as_str(), settlement.awarded_points))
            .collect::<Vec<_>>();
        ranked.sort_by(|(a_node_id, a_points), (b_node_id, b_points)| {
            b_points
                .cmp(a_points)
                .then_with(|| a_node_id.cmp(b_node_id))
        });
        let mut index = 0_usize;
        while remainder > 0 && !ranked.is_empty() {
            let node_id = ranked[index % ranked.len()].0;
            if let Some(cap) = budget.node_credit_caps.get_mut(node_id) {
                *cap = cap.saturating_add(1);
                remainder -= 1;
            }
            index = index.saturating_add(1);
        }
    }

    fn cap_minted_credits_by_system_order_budget(
        &mut self,
        epoch_index: u64,
        node_id: &str,
        requested_credits: u64,
    ) -> u64 {
        let Some(budget) = self.state.system_order_pool_budgets.get_mut(&epoch_index) else {
            return requested_credits;
        };
        if requested_credits == 0 || budget.remaining_credit_budget == 0 {
            return 0;
        }
        let node_cap = budget.node_credit_caps.get(node_id).copied().unwrap_or(0);
        let node_allocated = budget
            .node_credit_allocated
            .get(node_id)
            .copied()
            .unwrap_or(0);
        let node_remaining = node_cap.saturating_sub(node_allocated);
        if node_remaining == 0 {
            return 0;
        }
        let allowed = requested_credits
            .min(node_remaining)
            .min(budget.remaining_credit_budget);
        if allowed == 0 {
            return 0;
        }
        budget.remaining_credit_budget = budget.remaining_credit_budget.saturating_sub(allowed);
        budget
            .node_credit_allocated
            .entry(node_id.to_string())
            .and_modify(|value| *value = value.saturating_add(allowed))
            .or_insert(allowed);
        allowed
    }

    fn require_bound_node_identity(&self, node_id: &str) -> Result<&str, WorldError> {
        self.state
            .node_identity_bindings
            .get(node_id)
            .map(String::as_str)
            .ok_or_else(|| WorldError::ResourceBalanceInvalid {
                reason: format!("node identity is not bound: {node_id}"),
            })
    }

    pub fn mint_node_power_credits(
        &mut self,
        node_id: &str,
        amount: u64,
    ) -> Result<u64, WorldError> {
        let balance = self.node_asset_balance_entry_mut(node_id)?;
        balance.power_credit_balance = balance.power_credit_balance.saturating_add(amount);
        balance.total_minted_credits = balance.total_minted_credits.saturating_add(amount);
        Ok(balance.power_credit_balance)
    }

    pub fn burn_node_power_credits(
        &mut self,
        node_id: &str,
        amount: u64,
    ) -> Result<u64, WorldError> {
        let balance = self.node_asset_balance_entry_mut(node_id)?;
        if amount > balance.power_credit_balance {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!(
                    "insufficient power credits for {}: balance={} burn={}",
                    node_id, balance.power_credit_balance, amount
                ),
            });
        }
        balance.power_credit_balance -= amount;
        balance.total_burned_credits = balance.total_burned_credits.saturating_add(amount);
        Ok(balance.power_credit_balance)
    }

    fn node_asset_balance_entry_mut(
        &mut self,
        node_id: &str,
    ) -> Result<&mut NodeAssetBalance, WorldError> {
        if node_id.trim().is_empty() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "node_id cannot be empty".to_string(),
            });
        }
        Ok(self
            .state
            .node_asset_balances
            .entry(node_id.to_string())
            .or_insert_with(|| NodeAssetBalance {
                node_id: node_id.to_string(),
                ..NodeAssetBalance::default()
            }))
    }

    // ---------------------------------------------------------------------
    // Resource ledger
    // ---------------------------------------------------------------------

    pub fn resource_balance(&self, kind: ResourceKind) -> i64 {
        self.state.resources.get(&kind).copied().unwrap_or(0)
    }

    pub fn set_resource_balance(&mut self, kind: ResourceKind, amount: i64) {
        self.state.resources.insert(kind, amount);
    }

    pub fn adjust_resource_balance(
        &mut self,
        kind: ResourceKind,
        delta: i64,
    ) -> Result<i64, WorldError> {
        let entry = self.state.resources.entry(kind).or_insert(0);
        let next = entry
            .checked_add(delta)
            .ok_or_else(|| WorldError::ResourceBalanceInvalid {
                reason: format!(
                    "resource balance overflow: kind={kind:?} current={} delta={delta}",
                    *entry
                ),
            })?;
        *entry = next;
        Ok(*entry)
    }

    pub fn material_profile(&self, material_kind: &str) -> Option<&MaterialProfileV1> {
        self.state.material_profiles.get(material_kind)
    }

    pub fn product_profile(&self, product_id: &str) -> Option<&ProductProfileV1> {
        self.state.product_profiles.get(product_id)
    }

    pub fn recipe_profile(&self, recipe_id: &str) -> Option<&RecipeProfileV1> {
        self.state.recipe_profiles.get(recipe_id)
    }

    pub fn factory_profile(&self, factory_id: &str) -> Option<&FactoryProfileV1> {
        self.state.factory_profiles.get(factory_id)
    }

    pub fn upsert_material_profile(
        &mut self,
        profile: MaterialProfileV1,
    ) -> Result<(), WorldError> {
        if profile.kind.trim().is_empty() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "material profile kind cannot be empty".to_string(),
            });
        }
        if profile.tier == 0 {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!("material profile tier must be >= 1: {}", profile.kind),
            });
        }
        self.state
            .material_profiles
            .insert(profile.kind.clone(), profile);
        Ok(())
    }

    pub fn upsert_product_profile(&mut self, profile: ProductProfileV1) -> Result<(), WorldError> {
        if profile.product_id.trim().is_empty() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "product profile product_id cannot be empty".to_string(),
            });
        }
        self.state
            .product_profiles
            .insert(profile.product_id.clone(), profile);
        Ok(())
    }

    pub fn upsert_recipe_profile(&mut self, profile: RecipeProfileV1) -> Result<(), WorldError> {
        if profile.recipe_id.trim().is_empty() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "recipe profile recipe_id cannot be empty".to_string(),
            });
        }
        self.state
            .recipe_profiles
            .insert(profile.recipe_id.clone(), profile);
        Ok(())
    }

    pub fn upsert_factory_profile(&mut self, profile: FactoryProfileV1) -> Result<(), WorldError> {
        if profile.factory_id.trim().is_empty() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "factory profile factory_id cannot be empty".to_string(),
            });
        }
        if profile.tier == 0 {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!("factory profile tier must be >= 1: {}", profile.factory_id),
            });
        }
        if profile.recipe_slots == 0 {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!(
                    "factory profile recipe_slots must be > 0: {}",
                    profile.factory_id
                ),
            });
        }
        self.state
            .factory_profiles
            .insert(profile.factory_id.clone(), profile);
        Ok(())
    }

    pub fn material_balance(&self, material_kind: &str) -> i64 {
        self.ledger_material_balance(&MaterialLedgerId::world(), material_kind)
    }

    pub fn ledger_material_balance(
        &self,
        ledger_id: &MaterialLedgerId,
        material_kind: &str,
    ) -> i64 {
        self.state
            .material_ledgers
            .get(ledger_id)
            .and_then(|ledger| ledger.get(material_kind))
            .copied()
            .unwrap_or_default()
    }

    pub fn has_materials_in_ledger(
        &self,
        ledger_id: &MaterialLedgerId,
        consume: &[MaterialStack],
    ) -> bool {
        consume.iter().all(|stack| {
            stack.amount > 0
                && self.ledger_material_balance(ledger_id, stack.kind.as_str()) >= stack.amount
        })
    }

    pub fn ledger_material_stacks(&self, ledger_id: &MaterialLedgerId) -> Vec<MaterialStack> {
        self.state
            .material_ledgers
            .get(ledger_id)
            .map(|ledger| {
                ledger
                    .iter()
                    .filter(|(_, amount)| **amount > 0)
                    .map(|(kind, amount)| MaterialStack::new(kind.clone(), *amount))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn set_material_balance(
        &mut self,
        material_kind: impl Into<String>,
        amount: i64,
    ) -> Result<(), WorldError> {
        self.set_ledger_material_balance(MaterialLedgerId::world(), material_kind, amount)
    }

    pub fn set_ledger_material_balance(
        &mut self,
        ledger_id: MaterialLedgerId,
        material_kind: impl Into<String>,
        amount: i64,
    ) -> Result<(), WorldError> {
        if amount < 0 {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!("material balance must be >= 0, got {amount}"),
            });
        }
        let material_kind = material_kind.into();
        if material_kind.trim().is_empty() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "material kind cannot be empty".to_string(),
            });
        }
        let ledger = self.state.material_ledgers.entry(ledger_id).or_default();
        if amount == 0 {
            ledger.remove(&material_kind);
        } else {
            ledger.insert(material_kind, amount);
        }
        self.sync_compat_world_materials_cache();
        Ok(())
    }

    pub fn adjust_material_balance(
        &mut self,
        material_kind: impl Into<String>,
        delta: i64,
    ) -> Result<i64, WorldError> {
        self.adjust_ledger_material_balance(MaterialLedgerId::world(), material_kind, delta)
    }

    pub fn adjust_ledger_material_balance(
        &mut self,
        ledger_id: MaterialLedgerId,
        material_kind: impl Into<String>,
        delta: i64,
    ) -> Result<i64, WorldError> {
        let material_kind = material_kind.into();
        if material_kind.trim().is_empty() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "material kind cannot be empty".to_string(),
            });
        }
        let current = self.ledger_material_balance(&ledger_id, material_kind.as_str());
        let next =
            current
                .checked_add(delta)
                .ok_or_else(|| WorldError::ResourceBalanceInvalid {
                    reason: format!(
                        "material balance overflow: kind={} current={} delta={}",
                        material_kind, current, delta
                    ),
                })?;
        if next < 0 {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!(
                    "material balance cannot be negative: kind={} current={} delta={}",
                    material_kind, current, delta
                ),
            });
        }
        let ledger = self.state.material_ledgers.entry(ledger_id).or_default();
        if next == 0 {
            ledger.remove(&material_kind);
        } else {
            ledger.insert(material_kind, next);
        }
        self.sync_compat_world_materials_cache();
        Ok(next)
    }

    pub fn transfer_material_between_ledgers(
        &mut self,
        from_ledger: &MaterialLedgerId,
        to_ledger: &MaterialLedgerId,
        material_kind: &str,
        amount: i64,
    ) -> Result<(), WorldError> {
        if amount <= 0 {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!("material transfer amount must be > 0, got {amount}"),
            });
        }
        let destination_current = self.ledger_material_balance(to_ledger, material_kind);
        destination_current.checked_add(amount).ok_or_else(|| {
            WorldError::ResourceBalanceInvalid {
                reason: format!(
                    "material balance overflow: kind={} current={} delta={}",
                    material_kind, destination_current, amount
                ),
            }
        })?;
        self.adjust_ledger_material_balance(
            from_ledger.clone(),
            material_kind.to_string(),
            -amount,
        )?;
        self.adjust_ledger_material_balance(to_ledger.clone(), material_kind.to_string(), amount)?;
        Ok(())
    }

    pub(super) fn apply_resource_delta(&mut self, delta: &ResourceDelta) -> Result<(), WorldError> {
        let mut planned = Vec::with_capacity(delta.entries.len());
        for (kind, amount) in &delta.entries {
            let current = self.resource_balance(*kind);
            let next = current.checked_add(*amount).ok_or_else(|| {
                WorldError::ResourceBalanceInvalid {
                    reason: format!(
                        "resource balance overflow: kind={kind:?} current={current} delta={amount}"
                    ),
                }
            })?;
            planned.push((*kind, next));
        }

        for (kind, next) in planned {
            self.state.resources.insert(kind, next);
        }
        Ok(())
    }

    pub fn agent_resource_balance(
        &self,
        agent_id: &str,
        kind: ResourceKind,
    ) -> Result<i64, WorldError> {
        let cell = self
            .state
            .agents
            .get(agent_id)
            .ok_or_else(|| WorldError::AgentNotFound {
                agent_id: agent_id.to_string(),
            })?;
        Ok(cell.state.resources.get(kind))
    }

    pub fn set_agent_resource_balance(
        &mut self,
        agent_id: &str,
        kind: ResourceKind,
        amount: i64,
    ) -> Result<(), WorldError> {
        let cell =
            self.state
                .agents
                .get_mut(agent_id)
                .ok_or_else(|| WorldError::AgentNotFound {
                    agent_id: agent_id.to_string(),
                })?;
        cell.state
            .resources
            .set(kind, amount)
            .map_err(|err| WorldError::ResourceBalanceInvalid {
                reason: format!("set resource failed: {err:?}"),
            })
    }

    pub fn adjust_agent_resource_balance(
        &mut self,
        agent_id: &str,
        kind: ResourceKind,
        delta: i64,
    ) -> Result<i64, WorldError> {
        let cell =
            self.state
                .agents
                .get_mut(agent_id)
                .ok_or_else(|| WorldError::AgentNotFound {
                    agent_id: agent_id.to_string(),
                })?;
        if delta >= 0 {
            cell.state.resources.add(kind, delta).map_err(|err| {
                WorldError::ResourceBalanceInvalid {
                    reason: format!("add resource failed: {err:?}"),
                }
            })?;
        } else {
            let amount = delta.saturating_abs();
            cell.state
                .resources
                .remove(kind, amount)
                .map_err(|err| match err {
                    StockError::NegativeAmount { .. }
                    | StockError::Insufficient { .. }
                    | StockError::Overflow { .. } => WorldError::ResourceBalanceInvalid {
                        reason: format!("remove resource failed: {err:?}"),
                    },
                })?;
        }
        Ok(cell.state.resources.get(kind))
    }

    fn sync_compat_world_materials_cache(&mut self) {
        self.state.materials = self
            .state
            .material_ledgers
            .get(&MaterialLedgerId::world())
            .cloned()
            .unwrap_or_default();
    }
}
