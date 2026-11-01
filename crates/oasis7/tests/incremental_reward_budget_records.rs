#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget, PageStoreError,
};
use oasis7_formal_protocol::{
    incremental_execution::*, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    NodeRewardMintRecord, SystemOrderPoolBudget, WorldState, incremental_field_member::*,
    incremental_field_read::*, incremental_reward_budget_records::*,
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
        _ => ValueTreeError::Invalid("reward fixture state"),
    }
}
fn source() -> WorldState {
    let mut world = WorldState::default();
    world.system_order_pool_budgets.insert(
        7,
        SystemOrderPoolBudget {
            epoch_index: 17,
            total_credit_budget: 0,
            remaining_credit_budget: u64::MAX,
            node_credit_caps: [("".into(), 0), (" Ω ".into(), u64::MAX)].into(),
            node_credit_allocated: [("".into(), u64::MAX), ("node".into(), 0)].into(),
        },
    );
    world.system_order_pool_budgets.insert(
        0,
        SystemOrderPoolBudget {
            epoch_index: u64::MAX,
            ..Default::default()
        },
    );
    let record = NodeRewardMintRecord {
        epoch_index: u64::MAX,
        node_id: " raw node Ω ".into(),
        source_awarded_points: 0,
        minted_power_credits: u64::MAX,
        settlement_hash: " raw hash ".into(),
        signer_node_id: " other signer ".into(),
        signature: " original signature ".into(),
    };
    world.reward_mint_records.push(record.clone());
    world.reward_mint_records.push(record);
    world.reward_mint_records.push(NodeRewardMintRecord {
        epoch_index: 0,
        signature: "".into(),
        ..Default::default()
    });
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
    let mut rows = Vec::with_capacity(16);
    let mut work = RewardRecordsWorkV1 {
        cells_remaining: 55,
        max_pending_node_bytes: 2 << 20,
    };
    prepare_reward_records_genesis_v1(
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
            assert!(rows.len() < 16);
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(work.cells_remaining, 0);
    assert_eq!(rows.len(), 10);
    let mut roots = RootSetV1::empty("reward-budget-test".into(), [0x38; 32]).unwrap();
    for ns in [StateNamespaceV1::Business, StateNamespaceV1::Authorization] {
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
#[test]
fn original_epochs_maps_and_ordered_duplicate_mints_cold_recover() {
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
        let family = qualify_reward_family_v1(
            &roots,
            &mut context,
            &mut scan(),
            &mut scan(),
            &mut 8192,
            1 << 20,
        )
        .unwrap();
        assert_eq!(family.record_counts(), [2, 3]);
        assert_eq!(family.mint_length(), 3);
        let _oracle = memory.reserve(256 << 10, MemoryClass::Recovery).unwrap();
        let original = source();
        for (key, columns) in [
            (
                RewardRecordKeyV1 {
                    kind: RewardRecordKindV1::Pool,
                    index: 7,
                },
                vec![
                    (RewardColumnV1::EpochIndex, RewardValueV1::Unsigned(17), ""),
                    (
                        RewardColumnV1::TotalCreditBudget,
                        RewardValueV1::Unsigned(0),
                        "",
                    ),
                    (
                        RewardColumnV1::RemainingCreditBudget,
                        RewardValueV1::Unsigned(u64::MAX),
                        "",
                    ),
                ],
            ),
            (
                RewardRecordKeyV1 {
                    kind: RewardRecordKindV1::Pool,
                    index: 0,
                },
                vec![(
                    RewardColumnV1::EpochIndex,
                    RewardValueV1::Unsigned(u64::MAX),
                    "",
                )],
            ),
            (
                RewardRecordKeyV1 {
                    kind: RewardRecordKindV1::Mint,
                    index: 0,
                },
                vec![
                    (
                        RewardColumnV1::EpochIndex,
                        RewardValueV1::Unsigned(u64::MAX),
                        "",
                    ),
                    (
                        RewardColumnV1::NodeId,
                        RewardValueV1::Text { utf8_bytes: 13 },
                        " raw node Ω ",
                    ),
                ],
            ),
            (
                RewardRecordKeyV1 {
                    kind: RewardRecordKindV1::Mint,
                    index: 1,
                },
                vec![(
                    RewardColumnV1::Signature,
                    RewardValueV1::Text { utf8_bytes: 20 },
                    " original signature ",
                )],
            ),
            (
                RewardRecordKeyV1 {
                    kind: RewardRecordKindV1::Mint,
                    index: 2,
                },
                vec![
                    (RewardColumnV1::EpochIndex, RewardValueV1::Unsigned(0), ""),
                    (
                        RewardColumnV1::Signature,
                        RewardValueV1::Text { utf8_bytes: 0 },
                        "",
                    ),
                ],
            ),
        ] {
            context.root = roots
                .state
                .iter()
                .find(|r| r.namespace == key.kind.namespace())
                .unwrap();
            let record = open_qualified_reward_record_v1(key, &roots, &family, &mut context)
                .unwrap()
                .unwrap();
            for (column, expected, text) in columns {
                let mut out = Vec::new();
                let _out = memory.reserve(128, MemoryClass::Recovery).unwrap();
                let value = read_qualified_reward_cell_v1(
                    RewardCellRequestV1 {
                        key,
                        cell: RewardCellV1::Column(column),
                        qualification: &record,
                    },
                    &mut context,
                    128,
                    &mut out,
                )
                .unwrap();
                assert_eq!(value, Some(expected));
                assert_eq!(out, text.as_bytes());
            }
            if key.kind == RewardRecordKindV1::Pool {
                for (c, expected) in [
                    (
                        RewardCollectionV1::NodeCreditCaps,
                        &original.system_order_pool_budgets[&key.index].node_credit_caps,
                    ),
                    (
                        RewardCollectionV1::NodeCreditAllocated,
                        &original.system_order_pool_budgets[&key.index].node_credit_allocated,
                    ),
                ] {
                    for (id, value) in expected {
                        assert_eq!(
                            read_qualified_reward_cell_v1(
                                RewardCellRequestV1 {
                                    key,
                                    cell: RewardCellV1::Member(c, id),
                                    qualification: &record
                                },
                                &mut context,
                                8,
                                &mut std::io::sink()
                            )
                            .unwrap(),
                            Some(RewardValueV1::Unsigned(*value))
                        );
                    }
                }
            }
        }
    }
}

#[path = "support/incremental_reward_budget_operations.rs"]
mod operations;

#[path = "support/incremental_reward_budget_oracle.rs"]
mod oracle;

#[path = "support/incremental_reward_budget_faults.rs"]
mod faults;

#[path = "support/incremental_reward_budget_pages.rs"]
mod pages;

#[path = "support/incremental_reward_budget_malformed.rs"]
mod malformed;

#[path = "support/incremental_reward_budget_scaling.rs"]
mod scaling;

#[path = "support/incremental_reward_budget_genesis.rs"]
mod genesis;
