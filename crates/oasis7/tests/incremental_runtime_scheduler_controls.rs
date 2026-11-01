#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget, PageStoreError,
};
use oasis7_formal_protocol::{
    incremental_execution::*, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_runtime::{
    ModuleTickRoutingDeterministicSnapshot, WorldRuntimeBackpressureStats,
    WorldRuntimeMemoryLimits, incremental_field_member::*, incremental_field_read::*,
    incremental_runtime_scheduler_controls::*,
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical};
fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 1_250_000,
        ..fixture::physical()
    }
}
fn state_error(error: PageStoreError) -> ValueTreeError {
    match error {
        PageStoreError::Memory(v) => ValueTreeError::Memory(v),
        PageStoreError::ResourceLimited(v) => ValueTreeError::ResourceLimited(v),
        PageStoreError::Cancelled => ValueTreeError::Cancelled,
        PageStoreError::Io(v) => ValueTreeError::Io(v),
        _ => ValueTreeError::Invalid("fixed economy test state"),
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100000,
        leaves_remaining: 1000,
        read_bytes_remaining: 64 << 20,
    }
}
fn limits_source() -> WorldRuntimeMemoryLimits {
    WorldRuntimeMemoryLimits {
        max_pending_actions: usize::MAX,
        max_pending_effects: 0,
        max_inflight_effects: 17,
        max_journal_events: 93,
    }
}
fn stats_source() -> WorldRuntimeBackpressureStats {
    WorldRuntimeBackpressureStats {
        pending_actions_evicted: u64::MAX,
        pending_effects_evicted: 0,
        inflight_effects_evicted: 19,
        inflight_effect_dispatch_blocked: 71,
        journal_events_evicted: 72,
    }
}
fn routing_source() -> ModuleTickRoutingDeterministicSnapshot {
    ModuleTickRoutingDeterministicSnapshot {
        last_due_count: usize::MAX,
        last_invoked_count: 0,
        last_missing_invocation_count: 23,
        missing_invocation_count: u64::MAX,
        oldest_overdue_ticks: Some(0),
        routing_count: 29,
    }
}
fn limits() -> MemberRecordLimitsV1 {
    MemberRecordLimitsV1 {
        semantic_bytes: 1024,
        payload_bytes: 165,
        record_bytes: 4096,
    }
}
fn seed(store: &IncrementalPageStore, memory: &MemoryBudget) -> RootSetV1 {
    seed_cursor(store, memory, Some(""))
}
fn seed_cursor(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    cursor: Option<&str>,
) -> RootSetV1 {
    let _caller = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let mut writer = store.value_session(physical());
    let mut rows = Vec::with_capacity(16);
    let mut work = RuntimeSchedulerControlsWorkV1 {
        cells_remaining: 24,
        max_pending_node_bytes: 2 << 20,
    };
    prepare_runtime_scheduler_controls_genesis_v1(
        &RuntimeSchedulerControlsRefV1 {
            cursor,
            limits: &limits_source(),
            backpressure: &stats_source(),
            routing: &routing_source(),
        },
        &mut MemberRecordWriteContext {
            sink: &mut writer,
            memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: MemberRecordLimitsV1 {
                payload_bytes: 165.max(cursor.map_or(0, |s| s.len() as u64)),
                record_bytes: 4096.max(cursor.map_or(0, |s| s.len() as u64 + 4096)),
                ..limits()
            },
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
    assert_eq!(rows.len(), 16);
    let mut roots = RootSetV1::empty("scheduler-controls".into(), [0x63; 32]).unwrap();
    {
        let namespace = StateNamespaceV1::Scheduler;
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let mut writes = rows
            .iter()
            .filter(|row| row.namespace == namespace)
            .map(|row| StateWriteV1 {
                key: row.mutation.key.clone(),
                before: None,
                after: row.mutation.after,
            })
            .collect::<Vec<_>>();
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        let charge = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
        let delta = charge.retain(
            StateDeltaV1::build(
                StateRootV1::empty(namespace).unwrap(),
                writes,
                &state,
                2 << 20,
            )
            .unwrap(),
        );
        writer.put_state_pages(&delta, memory).unwrap();
        *roots
            .state
            .iter_mut()
            .find(|root| root.namespace == namespace)
            .unwrap() = delta.after.clone();
    }
    roots
}
macro_rules! read_ctx {
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
            semantic_bytes: 1024,
            record_bytes: 4096,
            take_state_error: || $state.take_read_error().map(state_error),
            cancelled: || false,
        }
    };
}

#[test]
fn scheduler_controls_root_only_cold_original_values() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let canonical = {
            let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            seed(&store, &memory).canonical_bytes().unwrap()
        };
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let _caller = memory.reserve(128 << 10, MemoryClass::Recovery).unwrap();
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Recovery)
                .unwrap();
        let roots = RootSetV1::from_canonical_bytes(&canonical).unwrap();
        let state = store
            .state_session(physical(), MemoryClass::Recovery)
            .unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = read_ctx!(
            roots
                .state
                .iter()
                .find(|r| r.namespace == StateNamespaceV1::Scheduler)
                .unwrap(),
            &state,
            &values,
            &memory,
            MemoryClass::Recovery,
            &mut io,
            &mut keys
        );
        let family = qualify_runtime_scheduler_controls_v1(
            &roots,
            &mut ctx,
            &mut scan(),
            &mut scan(),
            &mut 1000,
            32,
        )
        .unwrap();
        family.validate_binding(&roots.digest().unwrap()).unwrap();
        assert!(family.validate_binding(&[0; 32]).is_err());
        use RuntimeSchedulerControlValueV1 as V;
        use RuntimeSchedulerScalarV1 as S;
        // Independent original column/value oracle, including empty Some cursor.
        for (field, want) in [
            (S::Cursor, V::Text { utf8_bytes: 0 }),
            (S::MaxPendingActions, V::Unsigned(usize::MAX as u64)),
            (S::MaxPendingEffects, V::Unsigned(0)),
            (S::MaxInflightEffects, V::Unsigned(17)),
            (S::PendingActionsEvicted, V::Unsigned(u64::MAX)),
            (S::PendingEffectsEvicted, V::Unsigned(0)),
            (S::InflightEffectsEvicted, V::Unsigned(19)),
        ] {
            assert_eq!(
                read_qualified_runtime_scheduler_scalar_v1(
                    field,
                    &roots,
                    &family,
                    &mut ctx,
                    32,
                    &mut std::io::sink()
                )
                .unwrap(),
                want
            );
        }
        let routing = open_qualified_runtime_routing_v1(&roots, &family, &mut ctx).unwrap();
        assert_eq!(routing.root().leaf_count, 8);
        use RuntimeRoutingColumnV1 as C;
        for (column, want) in [
            (C::LastDueCount, usize::MAX as u64),
            (C::LastInvokedCount, 0),
            (C::LastMissingInvocationCount, 23),
            (C::MissingInvocationCount, u64::MAX),
            (C::OldestOverdueTicks, 0),
            (C::RoutingCount, 29),
        ] {
            assert_eq!(
                read_qualified_runtime_routing_column_v1(column, &roots, &family, &mut ctx)
                    .unwrap(),
                V::Unsigned(want)
            );
        }
    }
}

#[path = "support/incremental_runtime_scheduler_operations.rs"]
mod operations;

#[path = "support/incremental_runtime_scheduler_schema.rs"]
mod schema;

#[path = "support/incremental_runtime_scheduler_pages.rs"]
mod pages;

#[path = "support/incremental_runtime_scheduler_faults.rs"]
mod faults;

#[path = "support/incremental_runtime_scheduler_scaling.rs"]
mod scaling;

#[path = "support/incremental_runtime_scheduler_literal.rs"]
mod literal;

#[path = "support/incremental_runtime_scheduler_genesis.rs"]
mod genesis;
