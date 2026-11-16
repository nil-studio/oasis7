use super::fixture;
use oasis7_distfs::incremental_page_store::{PageIoBudget, PageStoreError};
use oasis7_formal_protocol::{
    incremental_state::StateScanBudget, incremental_value::ValueTreeError,
};
use oasis7_runtime::WorldState;
use oasis7_runtime::{
    MainTokenBurnPolicy, MainTokenConfig, MainTokenEpochIssuanceRecord,
    MainTokenGenesisAllocationBucketState, MainTokenInflationPolicy, MainTokenIssuanceSplitPolicy,
    MainTokenScheduledPolicyUpdate, RestrictedStarterClaimGrantState,
    RestrictedStarterClaimGrantStatus,
};

pub(crate) fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 2_000_000,
        ..fixture::physical()
    }
}

pub(crate) fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100_000,
        leaves_remaining: 1000,
        read_bytes_remaining: 64 << 20,
    }
}

pub(crate) fn state_error(e: PageStoreError) -> ValueTreeError {
    match e {
        PageStoreError::Io(e) => ValueTreeError::Io(e),
        PageStoreError::Memory(e) => ValueTreeError::Memory(e),
        PageStoreError::ResourceLimited(e) => ValueTreeError::ResourceLimited(e),
        PageStoreError::Cancelled => ValueTreeError::Cancelled,
        _ => ValueTreeError::Invalid("main token policy fixture state"),
    }
}

pub(crate) fn source() -> WorldState {
    let mut world = WorldState {
        main_token_config: MainTokenConfig {
            symbol: " Ω ".into(),
            decimals: u8::MAX,
            initial_supply: u64::MAX,
            max_supply: Some(0),
            inflation_policy: MainTokenInflationPolicy {
                base_rate_bps: 1,
                min_rate_bps: 2,
                max_rate_bps: 3,
                target_stake_ratio_bps: 4,
                stake_feedback_gain_bps: 5,
                epochs_per_year: u32::MAX,
            },
            issuance_split: MainTokenIssuanceSplitPolicy {
                staking_reward_bps: 6,
                node_service_reward_bps: 7,
                ecosystem_pool_bps: 8,
                security_reserve_bps: 9,
            },
            burn_policy: MainTokenBurnPolicy {
                gas_base_fee_burn_bps: 10,
                slash_burn_bps: 11,
                module_fee_burn_bps: 12,
            },
        },
        ..Default::default()
    };
    world.main_token_genesis_buckets.insert(
        "".into(),
        MainTokenGenesisAllocationBucketState {
            bucket_id: " other bucket ".into(),
            ratio_bps: u32::MAX,
            recipient: " Ω ".into(),
            cliff_epochs: 0,
            linear_unlock_epochs: 1,
            start_epoch: u64::MAX,
            allocated_amount: 2,
            claimed_amount: 3,
        },
    );
    world.main_token_epoch_issuance_records.insert(
        7,
        MainTokenEpochIssuanceRecord {
            epoch_index: 17,
            inflation_rate_bps: u32::MAX,
            issued_amount: 0,
            staking_reward_amount: 1,
            node_service_reward_amount: 2,
            ecosystem_pool_amount: 3,
            security_reserve_amount: u64::MAX,
        },
    );
    let grant = RestrictedStarterClaimGrantState {
        beneficiary_account_id: " other beneficiary ".into(),
        issuer_id: "issuer".into(),
        issuance_reason: "reason".into(),
        spend_scope: "scope".into(),
        source_treasury_bucket_id: "treasury".into(),
        issued_amount: u64::MAX,
        issued_at_epoch: 0,
        expires_at_epoch: 1,
        status: RestrictedStarterClaimGrantStatus::Revoked,
        status_updated_at_epoch: Some(0),
        status_reason: Some("".into()),
    };
    world
        .restricted_starter_claim_grants
        .insert(" raw grant ".into(), grant.clone());
    world.restricted_starter_claim_grants.insert(
        "none".into(),
        RestrictedStarterClaimGrantState {
            status: RestrictedStarterClaimGrantStatus::Issued,
            status_updated_at_epoch: None,
            status_reason: None,
            ..grant
        },
    );
    let next = MainTokenConfig {
        symbol: "".into(),
        decimals: 0,
        initial_supply: 0,
        max_supply: None,
        ..world.main_token_config.clone()
    };
    world.main_token_scheduled_policy_updates.insert(
        9,
        MainTokenScheduledPolicyUpdate {
            proposal_id: 19,
            effective_epoch: 0,
            next_config: next,
        },
    );
    world
}

macro_rules! ctx {
    ($root:expr,$state:expr,$values:expr,$memory:expr,$class:expr,$io:expr,$keys:expr) => {
        WorldFieldReadContextV1 {
            root: $root,
            state: $state,
            values: ValueReadContext {
                source: $values,
                memory: $memory,
                class: $class,
                io: $io,
            },
            key_work: $keys,
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            take_state_error: || $state.take_read_error().map(state_error),
            cancelled: || false,
        }
    };
}
pub(crate) use ctx;
