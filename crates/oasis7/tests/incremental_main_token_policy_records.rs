#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget, PageStoreError,
};
use oasis7_formal_protocol::{
    incremental_execution::*, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    WorldState, incremental_field_member::*, incremental_field_read::*,
    incremental_main_token_policy_records::*,
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical};
fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 2_000_000,
        ..fixture::physical()
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100_000,
        leaves_remaining: 1000,
        read_bytes_remaining: 64 << 20,
    }
}
fn state_error(e: PageStoreError) -> ValueTreeError {
    match e {
        PageStoreError::Io(e) => ValueTreeError::Io(e),
        PageStoreError::Memory(e) => ValueTreeError::Memory(e),
        PageStoreError::ResourceLimited(e) => ValueTreeError::ResourceLimited(e),
        PageStoreError::Cancelled => ValueTreeError::Cancelled,
        _ => ValueTreeError::Invalid("main token policy fixture state"),
    }
}
use oasis7_runtime::{
    MainTokenBurnPolicy, MainTokenConfig, MainTokenEpochIssuanceRecord,
    MainTokenGenesisAllocationBucketState, MainTokenInflationPolicy, MainTokenIssuanceSplitPolicy,
    MainTokenScheduledPolicyUpdate, RestrictedStarterClaimGrantState,
    RestrictedStarterClaimGrantStatus,
};
fn source() -> WorldState {
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
fn limits() -> MemberRecordLimitsV1 {
    MemberRecordLimitsV1 {
        semantic_bytes: 4096,
        payload_bytes: 1 << 20,
        record_bytes: 2 << 20,
    }
}
fn seed(store: &IncrementalPageStore, memory: &MemoryBudget, source: &WorldState) -> RootSetV1 {
    let _caller = memory.reserve(256 << 10, MemoryClass::Ordinary).unwrap();
    let mut writer = store.value_session(physical());
    let mut rows = Vec::with_capacity(24);
    let mut work = MainTokenPolicyWorkV1 {
        cells_remaining: 101,
        max_pending_node_bytes: 2 << 20,
    };
    prepare_main_token_policy_genesis_v1(
        source,
        &mut MemberRecordWriteContext {
            sink: &mut writer,
            memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: limits(),
            cancelled: || false,
        },
        &mut work,
        &mut |row| {
            assert!(rows.len() < 24);
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(work.cells_remaining, 0);
    assert_eq!(rows.len(), 16);
    let mut roots = RootSetV1::empty("main-token-policy-test".into(), [0x40; 32]).unwrap();
    for ns in [
        StateNamespaceV1::Business,
        StateNamespaceV1::Authorization,
        StateNamespaceV1::Scheduler,
    ] {
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let mut writes = rows
            .iter()
            .filter(|row| row.namespace == ns)
            .map(|row| StateWriteV1 {
                key: row.mutation.key.clone(),
                before: None,
                after: row.mutation.after,
            })
            .collect::<Vec<_>>();
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        let delta = memory
            .reserve(8 << 20, MemoryClass::Ordinary)
            .unwrap()
            .retain(
                StateDeltaV1::build(StateRootV1::empty(ns).unwrap(), writes, &state, 2 << 20)
                    .unwrap(),
            );
        writer.put_state_pages(&delta, memory).unwrap();
        *roots.state.iter_mut().find(|r| r.namespace == ns).unwrap() = delta.after.clone();
    }
    roots
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

fn config_expected(
    next: bool,
) -> Vec<(
    MainTokenConfigColumnV1,
    MainTokenPolicyValueV1,
    &'static str,
)> {
    use MainTokenConfigColumnV1 as C;
    use MainTokenPolicyValueV1 as V;
    let mut cells = vec![
        (
            C::Symbol,
            V::Text {
                utf8_bytes: if next { 0 } else { 4 },
            },
            if next { "" } else { " Ω " },
        ),
        (C::Decimals, V::Unsigned(if next { 0 } else { 255 }), ""),
        (
            C::InitialSupply,
            V::Unsigned(if next { 0 } else { u64::MAX }),
            "",
        ),
        (
            C::MaxSupply,
            if next { V::Unit } else { V::Unsigned(0) },
            "",
        ),
    ];
    for (c, n) in [
        (C::BaseRateBps, 1),
        (C::MinRateBps, 2),
        (C::MaxRateBps, 3),
        (C::TargetStakeRatioBps, 4),
        (C::StakeFeedbackGainBps, 5),
        (C::EpochsPerYear, u64::from(u32::MAX)),
        (C::StakingRewardBps, 6),
        (C::NodeServiceRewardBps, 7),
        (C::EcosystemPoolBps, 8),
        (C::SecurityReserveBps, 9),
        (C::GasBaseFeeBurnBps, 10),
        (C::SlashBurnBps, 11),
        (C::ModuleFeeBurnBps, 12),
    ] {
        cells.push((c, V::Unsigned(n), ""))
    }
    cells
}
#[test]
fn all_original_policy_fields_cold_recover_without_source() {
    use MainTokenPolicyCellV1 as Cell;
    use MainTokenPolicyColumnV1 as C;
    use MainTokenPolicyKeyV1 as K;
    use MainTokenPolicyValueV1 as V;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("cold");
        let roots = {
            let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            seed(&store, &memory, &source())
        };
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Recovery)
                .unwrap();
        let state = store
            .state_session(physical(), MemoryClass::Recovery)
            .unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut context = ctx!(
            &roots.state[0],
            &state,
            &values,
            &memory,
            MemoryClass::Recovery,
            &mut io,
            &mut keys
        );
        let family = qualify_main_token_policy_family_v1(
            &roots,
            &mut context,
            &mut scan(),
            &mut scan(),
            &mut 8192,
            1 << 20,
        )
        .unwrap();
        assert_eq!(family.record_counts(), [1, 1, 1, 2, 1]);
        eprintln!(
            "bootstrap {compression:?}: state={:?} values={:?}",
            state.remaining_io().unwrap(),
            values.remaining_io().unwrap()
        );
        let _oracle = memory.reserve(512 << 10, MemoryClass::Recovery).unwrap();
        // Full family qualification and opens retain one continuous 2M session.
        // Each public point oracle below is a distinct bounded operation in the
        // same Recovery pool. Their cumulative physical work is reported separately.
        let mut point_totals = [0u64; 4];
        for key in [
            K::Config,
            K::Bucket(""),
            K::Issuance(7),
            K::Grant(" raw grant "),
            K::Grant("none"),
            K::Scheduled(9),
        ] {
            context.root = roots
                .state
                .iter()
                .find(|r| r.namespace == key.kind().namespace())
                .unwrap();
            let proof =
                open_qualified_main_token_policy_record_v1(key, &roots, &family, &mut context)
                    .unwrap()
                    .unwrap();
            let mut cells = vec![(Cell::Record, V::Unit, "")];
            match key {
                K::Config => cells.extend(
                    config_expected(false)
                        .into_iter()
                        .map(|(c, v, s)| (Cell::Config(c), v, s)),
                ),
                K::Scheduled(_) => {
                    cells.push((Cell::Column(C::ProposalId), V::Unsigned(19), ""));
                    cells.push((Cell::Column(C::EffectiveEpoch), V::Unsigned(0), ""));
                    cells.extend(
                        config_expected(true)
                            .into_iter()
                            .map(|(c, v, s)| (Cell::NextConfig(c), v, s)),
                    );
                }
                K::Bucket(_) => {
                    for (c, n) in [
                        (C::RatioBps, u64::from(u32::MAX)),
                        (C::CliffEpochs, 0),
                        (C::LinearUnlockEpochs, 1),
                        (C::StartEpoch, u64::MAX),
                        (C::AllocatedAmount, 2),
                        (C::ClaimedAmount, 3),
                    ] {
                        cells.push((Cell::Column(c), V::Unsigned(n), ""))
                    }
                    for (c, s) in [(C::BucketId, " other bucket "), (C::Recipient, " Ω ")] {
                        cells.push((
                            Cell::Column(c),
                            V::Text {
                                utf8_bytes: s.len() as u64,
                            },
                            s,
                        ))
                    }
                }
                K::Issuance(_) => {
                    for (c, n) in [
                        (C::EpochIndex, 17),
                        (C::InflationRateBps, u64::from(u32::MAX)),
                        (C::IssuedAmount, 0),
                        (C::StakingRewardAmount, 1),
                        (C::NodeServiceRewardAmount, 2),
                        (C::EcosystemPoolAmount, 3),
                        (C::SecurityReserveAmount, u64::MAX),
                    ] {
                        cells.push((Cell::Column(c), V::Unsigned(n), ""))
                    }
                }
                K::Grant(id) => {
                    for (c, s) in [
                        (C::BeneficiaryAccountId, " other beneficiary "),
                        (C::IssuerId, "issuer"),
                        (C::IssuanceReason, "reason"),
                        (C::SpendScope, "scope"),
                        (C::SourceTreasuryBucketId, "treasury"),
                        (C::Status, if id == "none" { "issued" } else { "revoked" }),
                    ] {
                        cells.push((
                            Cell::Column(c),
                            V::Text {
                                utf8_bytes: s.len() as u64,
                            },
                            s,
                        ))
                    }
                    for (c, n) in [
                        (C::IssuedAmount, u64::MAX),
                        (C::IssuedAtEpoch, 0),
                        (C::ExpiresAtEpoch, 1),
                    ] {
                        cells.push((Cell::Column(c), V::Unsigned(n), ""))
                    }
                    cells.push((
                        Cell::Column(C::StatusUpdatedAtEpoch),
                        if id == "none" {
                            V::Unit
                        } else {
                            V::Unsigned(0)
                        },
                        "",
                    ));
                    cells.push((
                        Cell::Column(C::StatusReason),
                        if id == "none" {
                            V::Unit
                        } else {
                            V::Text { utf8_bytes: 0 }
                        },
                        "",
                    ));
                }
            }
            for (cell, expected, text) in cells {
                let mut output = Vec::new();
                let point_state = store
                    .state_session(physical(), MemoryClass::Recovery)
                    .unwrap();
                let point_values = store.value_session(physical());
                let mut point_io = logical();
                let mut point_keys = SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                };
                let mut point_context = ctx!(
                    context.root,
                    &point_state,
                    &point_values,
                    &memory,
                    MemoryClass::Recovery,
                    &mut point_io,
                    &mut point_keys
                );
                assert_eq!(
                    read_qualified_main_token_policy_cell_v1(
                        MainTokenPolicyCellRequestV1 {
                            key,
                            cell,
                            qualification: &proof
                        },
                        &mut point_context,
                        128,
                        &mut output
                    )
                    .unwrap_or_else(|error| {
                        panic!("cold point {compression:?} {key:?} {cell:?}: {error:?}; state={:?}, values={:?}", point_state.remaining_io().unwrap(), point_values.remaining_io().unwrap())
                    }),
                    expected
                );
                assert_eq!(output, text.as_bytes());
                let remaining = point_state.remaining_io().unwrap();
                point_totals[0] += physical().reads_remaining - remaining.reads_remaining;
                point_totals[1] += physical().read_bytes_remaining - remaining.read_bytes_remaining;
                let remaining = point_values.remaining_io().unwrap();
                point_totals[2] += physical().reads_remaining - remaining.reads_remaining;
                point_totals[3] += physical().read_bytes_remaining - remaining.read_bytes_remaining;
            }
            eprintln!(
                "complete {compression:?} {key:?}: state={:?} values={:?}",
                state.remaining_io().unwrap(),
                values.remaining_io().unwrap()
            );
        }
        eprintln!(
            "independent public point totals {compression:?}: state_ops/bytes/value_ops/bytes={point_totals:?}; no single-session aggregate claim"
        );
    }
}

#[path = "support/incremental_main_token_policy_operations.rs"]
mod operations;

#[path = "support/incremental_main_token_policy_faults.rs"]
mod faults;

#[path = "support/incremental_main_token_policy_scaling.rs"]
mod scaling;

#[path = "support/incremental_main_token_policy_pages.rs"]
mod pages;

#[path = "support/incremental_main_token_policy_genesis.rs"]
mod genesis;

#[path = "support/incremental_main_token_policy_malformed.rs"]
mod malformed;

#[path = "support/incremental_main_token_policy_oracle.rs"]
mod oracle;

#[path = "support/incremental_main_token_policy_status_output.rs"]
mod status_output;
