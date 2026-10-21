use serde::{Deserialize, Serialize};

const DEFAULT_MAIN_TOKEN_SYMBOL: &str = "OC";
const DEFAULT_MAIN_TOKEN_DECIMALS: u8 = 9;
pub const FROZEN_MAIN_TOKEN_INITIAL_SUPPLY: u64 = 10_000_000_000;
pub const MAIN_TOKEN_BPS_DENOMINATOR: u32 = 10_000;
pub const MAIN_TOKEN_NODE_ACCOUNT_PREFIX: &str = "oc:pk:";

pub const MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD: &str = "staking_reward_pool";
pub const MAIN_TOKEN_TREASURY_BUCKET_NODE_SERVICE_REWARD: &str = "node_service_reward_pool";
pub const MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL: &str = "ecosystem_pool";
pub const MAIN_TOKEN_TREASURY_BUCKET_RESTRICTED_STARTER_CLAIM_LIVEOPS_POOL: &str =
    "restricted_starter_claim_liveops_pool";
pub const MAIN_TOKEN_TREASURY_BUCKET_SECURITY_RESERVE: &str = "security_reserve";
pub const MAIN_TOKEN_TREASURY_BUCKET_GAS_FEE: &str = "gas_fee_treasury";
pub const MAIN_TOKEN_TREASURY_BUCKET_SLASH: &str = "slash_treasury";
pub const MAIN_TOKEN_TREASURY_BUCKET_MODULE_FEE: &str = "module_fee_treasury";
pub const RESTRICTED_STARTER_CLAIM_GRANT_SPEND_SCOPE_SLOT_1_ONLY: &str = "slot_1_claim_and_upkeep";

pub fn production_hardened_main_token_config() -> MainTokenConfig {
    MainTokenConfig {
        symbol: DEFAULT_MAIN_TOKEN_SYMBOL.to_string(),
        decimals: DEFAULT_MAIN_TOKEN_DECIMALS,
        initial_supply: FROZEN_MAIN_TOKEN_INITIAL_SUPPLY,
        max_supply: None,
        inflation_policy: MainTokenInflationPolicy::default(),
        issuance_split: MainTokenIssuanceSplitPolicy::default(),
        burn_policy: MainTokenBurnPolicy::default(),
    }
}

pub use oasis7_agent_api::world_action::MainTokenInflationPolicy;

pub use oasis7_agent_api::world_action::MainTokenIssuanceSplitPolicy;

pub use oasis7_agent_api::world_action::MainTokenBurnPolicy;

pub use oasis7_agent_api::world_action::MainTokenConfig;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MainTokenSupplyState {
    pub total_supply: u64,
    pub circulating_supply: u64,
    pub total_issued: u64,
    pub total_burned: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MainTokenAccountBalance {
    pub account_id: String,
    pub liquid_balance: u64,
    pub vested_balance: u64,
    #[serde(default)]
    pub restricted_starter_claim_balance: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RestrictedStarterClaimGrantStatus {
    #[default]
    Issued,
    Expired,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RestrictedStarterClaimGrantState {
    pub beneficiary_account_id: String,
    pub issuer_id: String,
    pub issuance_reason: String,
    pub spend_scope: String,
    pub source_treasury_bucket_id: String,
    pub issued_amount: u64,
    pub issued_at_epoch: u64,
    pub expires_at_epoch: u64,
    #[serde(default)]
    pub status: RestrictedStarterClaimGrantStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_updated_at_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RestrictedStarterClaimRefundSink {
    #[default]
    BeneficiaryRestrictedBalance,
    SourceTreasuryBucket,
}

pub use oasis7_agent_api::world_action::MainTokenGenesisAllocationPlan;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MainTokenGenesisAllocationBucketState {
    pub bucket_id: String,
    pub ratio_bps: u32,
    pub recipient: String,
    pub cliff_epochs: u64,
    pub linear_unlock_epochs: u64,
    pub start_epoch: u64,
    pub allocated_amount: u64,
    pub claimed_amount: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MainTokenEpochIssuanceRecord {
    pub epoch_index: u64,
    pub inflation_rate_bps: u32,
    pub issued_amount: u64,
    pub staking_reward_amount: u64,
    pub node_service_reward_amount: u64,
    pub ecosystem_pool_amount: u64,
    pub security_reserve_amount: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MainTokenScheduledPolicyUpdate {
    pub proposal_id: u64,
    pub effective_epoch: u64,
    pub next_config: MainTokenConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MainTokenNodePointsBridgeDistribution {
    pub node_id: String,
    pub account_id: String,
    pub amount: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MainTokenNodePointsBridgeEpochRecord {
    pub epoch_index: u64,
    pub settlement_hash: String,
    pub total_amount: u64,
    pub distributions: Vec<MainTokenNodePointsBridgeDistribution>,
}

pub use oasis7_agent_api::world_action::MainTokenTreasuryDistribution;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MainTokenTreasuryDistributionRecord {
    pub proposal_id: u64,
    pub distribution_id: String,
    pub bucket_id: String,
    pub total_amount: u64,
    pub distributions: Vec<MainTokenTreasuryDistribution>,
    pub distributed_epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RestrictedStarterClaimLiveopsPoolTopUpRecord {
    pub controller_account_id: String,
    pub top_up_id: String,
    pub source_treasury_bucket_id: String,
    pub target_treasury_bucket_id: String,
    pub amount: u64,
    pub topped_up_at_epoch: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MainTokenEconomyAuditThresholds {
    pub max_net_flow_bps_of_total_supply: u32,
    pub max_epoch_issued_bps_of_total_supply: u32,
    pub max_treasury_distribution_bps_of_total_supply: u32,
}

impl Default for MainTokenEconomyAuditThresholds {
    fn default() -> Self {
        Self {
            max_net_flow_bps_of_total_supply: 2_500,
            max_epoch_issued_bps_of_total_supply: 1_000,
            max_treasury_distribution_bps_of_total_supply: 1_200,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MainTokenEconomyAnomalyAlert {
    pub alert_id: String,
    pub metric: String,
    pub observed_bps: u32,
    pub threshold_bps: u32,
    pub exploit_signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MainTokenEconomyAuditReport {
    pub epoch_index: u64,
    pub mint_total: u64,
    pub burn_total: u64,
    pub net_flow: i128,
    pub issued_this_epoch: u64,
    pub treasury_distributed_this_epoch: u64,
    pub net_flow_bps_of_total_supply: u32,
    pub epoch_issued_bps_of_total_supply: u32,
    pub treasury_distribution_bps_of_total_supply: u32,
    pub alerts: Vec<MainTokenEconomyAnomalyAlert>,
}

impl MainTokenEconomyAuditReport {
    pub fn gate_passed(&self) -> bool {
        self.alerts.is_empty()
    }
}

pub fn validate_main_token_config_bounds(config: &MainTokenConfig) -> Result<(), String> {
    if config.symbol.trim().is_empty() {
        return Err("main token symbol cannot be empty".to_string());
    }
    if config.decimals > 18 {
        return Err(format!(
            "main token decimals must be <= 18, got {}",
            config.decimals
        ));
    }
    if let Some(max_supply) = config.max_supply
        && max_supply < config.initial_supply
    {
        return Err(format!(
            "main token max_supply must be >= initial_supply: max={} initial={}",
            max_supply, config.initial_supply
        ));
    }

    let inflation = &config.inflation_policy;
    if inflation.min_rate_bps > inflation.max_rate_bps {
        return Err(format!(
            "main token inflation min_rate_bps > max_rate_bps: {} > {}",
            inflation.min_rate_bps, inflation.max_rate_bps
        ));
    }
    if inflation.base_rate_bps < inflation.min_rate_bps
        || inflation.base_rate_bps > inflation.max_rate_bps
    {
        return Err(format!(
            "main token inflation base_rate_bps must be within [{}, {}], got {}",
            inflation.min_rate_bps, inflation.max_rate_bps, inflation.base_rate_bps
        ));
    }
    if inflation.max_rate_bps > MAIN_TOKEN_BPS_DENOMINATOR {
        return Err(format!(
            "main token inflation max_rate_bps must be <= {}, got {}",
            MAIN_TOKEN_BPS_DENOMINATOR, inflation.max_rate_bps
        ));
    }
    if inflation.target_stake_ratio_bps > MAIN_TOKEN_BPS_DENOMINATOR {
        return Err(format!(
            "main token target_stake_ratio_bps must be <= {}, got {}",
            MAIN_TOKEN_BPS_DENOMINATOR, inflation.target_stake_ratio_bps
        ));
    }
    if inflation.stake_feedback_gain_bps > MAIN_TOKEN_BPS_DENOMINATOR {
        return Err(format!(
            "main token stake_feedback_gain_bps must be <= {}, got {}",
            MAIN_TOKEN_BPS_DENOMINATOR, inflation.stake_feedback_gain_bps
        ));
    }
    if inflation.epochs_per_year == 0 {
        return Err("main token inflation epochs_per_year must be > 0".to_string());
    }

    let split = &config.issuance_split;
    if split.staking_reward_bps > MAIN_TOKEN_BPS_DENOMINATOR
        || split.node_service_reward_bps > MAIN_TOKEN_BPS_DENOMINATOR
        || split.ecosystem_pool_bps > MAIN_TOKEN_BPS_DENOMINATOR
        || split.security_reserve_bps > MAIN_TOKEN_BPS_DENOMINATOR
    {
        return Err(format!(
            "main token issuance split bps must each be <= {}",
            MAIN_TOKEN_BPS_DENOMINATOR
        ));
    }
    let split_sum = split
        .staking_reward_bps
        .saturating_add(split.node_service_reward_bps)
        .saturating_add(split.ecosystem_pool_bps)
        .saturating_add(split.security_reserve_bps);
    if split_sum != MAIN_TOKEN_BPS_DENOMINATOR {
        return Err(format!(
            "main token issuance split sum must be {}, got {}",
            MAIN_TOKEN_BPS_DENOMINATOR, split_sum
        ));
    }

    let burn = &config.burn_policy;
    if burn.gas_base_fee_burn_bps > MAIN_TOKEN_BPS_DENOMINATOR
        || burn.slash_burn_bps > MAIN_TOKEN_BPS_DENOMINATOR
        || burn.module_fee_burn_bps > MAIN_TOKEN_BPS_DENOMINATOR
    {
        return Err(format!(
            "main token burn bps must each be <= {}",
            MAIN_TOKEN_BPS_DENOMINATOR
        ));
    }

    Ok(())
}

pub fn main_token_account_id_from_node_public_key(public_key_hex: &str) -> String {
    let normalized = public_key_hex.trim().to_ascii_lowercase();
    format!("{MAIN_TOKEN_NODE_ACCOUNT_PREFIX}{normalized}")
}

pub fn is_main_token_treasury_distribution_bucket(bucket_id: &str) -> bool {
    matches!(
        bucket_id,
        MAIN_TOKEN_TREASURY_BUCKET_STAKING_REWARD
            | MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL
            | MAIN_TOKEN_TREASURY_BUCKET_SECURITY_RESERVE
    )
}

pub fn main_token_bucket_unlocked_amount(
    bucket: &MainTokenGenesisAllocationBucketState,
    current_epoch: u64,
) -> u64 {
    let unlock_start_epoch = bucket.start_epoch.saturating_add(bucket.cliff_epochs);
    if current_epoch < unlock_start_epoch {
        return 0;
    }
    if bucket.linear_unlock_epochs == 0 {
        return bucket.allocated_amount;
    }
    let elapsed = current_epoch.saturating_sub(unlock_start_epoch);
    if elapsed >= bucket.linear_unlock_epochs {
        return bucket.allocated_amount;
    }
    bucket.allocated_amount.saturating_mul(elapsed) / bucket.linear_unlock_epochs
}
