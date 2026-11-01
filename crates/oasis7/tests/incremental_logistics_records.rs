#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{IncrementalPageStore, PageCompression, PageIoBudget};
use oasis7_formal_protocol::{
    incremental_execution::*, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    MaterialLedgerId, MaterialTransitPriority, WorldState, incremental_field_member::*,
    incremental_field_overlay::*, incremental_field_read::*, incremental_logistics_records::*,
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, physical};
fn physical_scope() -> PageIoBudget {
    let mut io = physical();
    io.reads_remaining = 2_000_000;
    io
}
fn logical() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        write_bytes_remaining: 256 << 20,
    }
}
fn source() -> WorldState {
    let mut world = WorldState::default();
    let route = oasis7_runtime::LogisticsRouteV1 {
        route_id: " embedded/路线 ".into(),
        from_ledger: MaterialLedgerId::World,
        to_ledger: MaterialLedgerId::Factory(" raw/厂 ".into()),
        kind: " Ore ".into(),
        distance_km: i64::MIN,
        priority: MaterialTransitPriority::Urgent,
        owner_agent_id: " owner ".into(),
        available: false,
        capacity_units: i64::MAX,
        reserved_capacity_units: i64::MIN,
        tariff_electricity_per_unit: -1,
    };
    world.logistics_routes.insert(" outer/路线 ".into(), route);
    world.completed_logistics_paths.insert(
        " outer/path ".into(),
        serde_json::from_value(serde_json::json!({
            "path_id":" embedded path ","route_ids":["dup","dup",""],
            "from_ledger":"world","to_ledger":"world",
            "kind":" raw material ","settled_amount":i64::MIN,"remaining_recipe_amount":i64::MAX,
        }))
        .unwrap(),
    );
    world
        .completed_logistics_paths
        .get_mut(" outer/path ")
        .unwrap()
        .to_ledger = MaterialLedgerId::Agent(" target ".into());
    let mut empty = world.completed_logistics_paths[" outer/path "].clone();
    empty.route_ids.clear();
    empty.path_id.clear();
    world.completed_logistics_paths.insert("".into(), empty);
    world
}
fn work() -> LogisticsWorkBudgetV1 {
    LogisticsWorkBudgetV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 8 << 20,
        scan: StateScanBudget {
            nodes_remaining: 1_000_000,
            read_bytes_remaining: 256 << 20,
            leaves_remaining: 100_000,
        },
    }
}
fn seed(store: &IncrementalPageStore, memory: &MemoryBudget, source: &WorldState) -> RootSetV1 {
    let mut values = store.value_session(physical_scope());
    let _rows = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let mut rows: Vec<ReservedValue<ExecutionWriteV1>> = Vec::with_capacity(32);
    let mut remaining = 100_000;
    let count = prepare_logistics_records_genesis_v1(
        source,
        &mut MemberRecordWriteContext {
            sink: &mut values,
            memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 16 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        },
        &mut remaining,
        &mut work(),
        &mut |row| {
            assert!(rows.len() < 32);
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    let expected = source.logistics_routes.len() + source.completed_logistics_paths.len();
    assert_eq!(count, expected as u64);
    assert_eq!(rows.len(), 4 + expected);
    let mut roots = RootSetV1::empty("logistics-source".into(), [0x41; 32]).unwrap();
    for ns in [StateNamespaceV1::Business, StateNamespaceV1::Authorization] {
        let state = store
            .state_session(physical_scope(), MemoryClass::Ordinary)
            .unwrap();
        let mut writes: Vec<_> = rows
            .iter()
            .filter(|r| r.namespace == ns)
            .map(|r| StateWriteV1 {
                key: r.mutation.key.clone(),
                before: None,
                after: r.mutation.after,
            })
            .collect();
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        let charge = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
        let delta = charge.retain(
            StateDeltaV1::build(StateRootV1::empty(ns).unwrap(), writes, &state, 8 << 20).unwrap(),
        );
        values.put_state_pages(&delta, memory).unwrap();
        let index = roots.state.iter().position(|r| r.namespace == ns).unwrap();
        roots.state[index] = delta.after.clone();
    }
    roots
}

#[test]
fn logistics_original_records_qualify_and_point_read_after_cold_reopen() {
    use LogisticsColumnV1 as C;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let roots = {
            let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
            let _caller = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            seed(&store, &memory, &source())
        };
        for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
            let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
            let store =
                IncrementalPageStore::open(&path, memory.clone(), compression, class).unwrap();
            let _caller = memory.reserve(16 << 10, class).unwrap();
            let state = store.state_session(physical_scope(), class).unwrap();
            let values = store.value_session(physical_scope());
            let qualification = qualify_logistics_family_v1(
                &roots,
                &mut LogisticsFamilyReadContextV1 {
                    state: &state,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 16 << 20,
                    },
                    semantic_bytes: 1 << 20,
                    record_bytes: 2 << 20,
                    max_payload_bytes: 1 << 20,
                    outer_scan: &mut StateScanBudget {
                        nodes_remaining: 1_000_000,
                        read_bytes_remaining: 256 << 20,
                        leaves_remaining: 100_000,
                    },
                    take_state_error: || {
                        state
                            .take_read_error()
                            .map(|_| ValueTreeError::Invalid("logistics raw read"))
                    },
                    cancelled: || false,
                },
                &mut work(),
            )
            .unwrap();
            assert_eq!(qualification.record_counts(), [1, 2]);
            qualification
                .validate_binding(&roots.digest().unwrap())
                .unwrap();
            oracle::assert_complete_original_rows(&store, &memory, &roots, class);
            for (kind, id, cell, expected, text) in [
                (
                    LogisticsRecordKindV1::Route,
                    " outer/路线 ",
                    LogisticsCellV1::Column(C::RouteId),
                    LogisticsValueV1::Text {
                        utf8_bytes: " embedded/路线 ".len() as u64,
                    },
                    " embedded/路线 ",
                ),
                (
                    LogisticsRecordKindV1::Route,
                    " outer/路线 ",
                    LogisticsCellV1::Column(C::DistanceKm),
                    LogisticsValueV1::Signed(i64::MIN),
                    "",
                ),
                (
                    LogisticsRecordKindV1::Route,
                    " outer/路线 ",
                    LogisticsCellV1::Column(C::Priority),
                    LogisticsValueV1::Unsigned(1),
                    "",
                ),
                (
                    LogisticsRecordKindV1::Route,
                    " outer/路线 ",
                    LogisticsCellV1::Column(C::CapacityUnits),
                    LogisticsValueV1::Signed(i64::MAX),
                    "",
                ),
                (
                    LogisticsRecordKindV1::Path,
                    " outer/path ",
                    LogisticsCellV1::Column(C::SettledAmount),
                    LogisticsValueV1::Signed(i64::MIN),
                    "",
                ),
                (
                    LogisticsRecordKindV1::Path,
                    " outer/path ",
                    LogisticsCellV1::Collection(LogisticsCollectionV1::RouteIds),
                    LogisticsValueV1::Unsigned(3),
                    "",
                ),
                (
                    LogisticsRecordKindV1::Path,
                    " outer/path ",
                    LogisticsCellV1::TextItem(LogisticsCollectionV1::RouteIds, 1),
                    LogisticsValueV1::Text { utf8_bytes: 3 },
                    "dup",
                ),
                (
                    LogisticsRecordKindV1::Path,
                    "",
                    LogisticsCellV1::Collection(LogisticsCollectionV1::RouteIds),
                    LogisticsValueV1::Unsigned(0),
                    "",
                ),
            ] {
                let key = LogisticsRecordKeyV1 { kind, id };
                let root = roots
                    .state
                    .iter()
                    .find(|r| r.namespace == key.namespace())
                    .unwrap();
                let mut output = Vec::new();
                let _out = memory.reserve(128, class).unwrap();
                let actual = read_logistics_cell_v1(
                    key,
                    cell,
                    &mut WorldFieldReadContextV1 {
                        root,
                        state: &state,
                        values: ValueReadContext {
                            source: &values,
                            memory: &memory,
                            class,
                            io: &mut logical(),
                        },
                        key_work: &mut SemanticKeyBudget {
                            bytes_remaining: 4096,
                        },
                        semantic_bytes: 4096,
                        record_bytes: 8192,
                        take_state_error: || {
                            state
                                .take_read_error()
                                .map(|_| ValueTreeError::Invalid("logistics point read"))
                        },
                        cancelled: || false,
                    },
                    128,
                    &mut output,
                )
                .unwrap();
                assert_eq!(actual, LogisticsCellReadV1::Value(expected));
                assert_eq!(output, text.as_bytes());
            }
        }
    }
}

fn parent(roots: RootSetV1) -> ExecutionParent {
    ExecutionParent {
        commit: [0x34; 32],
        roots,
        authority: AuthorityBinding {
            token: [0x56; 32],
            epoch: 7,
        },
    }
}
fn tx_limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 128,
        max_writes: 16,
        max_facts: 8,
        max_pending_node_bytes: 8 << 20,
    }
}
fn qualify(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
    class: MemoryClass,
) -> ReservedValue<LogisticsFamilyQualificationV1> {
    let state = store.state_session(physical_scope(), class).unwrap();
    let values = store.value_session(physical_scope());
    qualify_logistics_family_v1(
        roots,
        &mut LogisticsFamilyReadContextV1 {
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class,
                io: &mut logical(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 16 << 20,
            },
            semantic_bytes: 4096,
            record_bytes: 8192,
            max_payload_bytes: 4096,
            outer_scan: &mut StateScanBudget {
                nodes_remaining: 1_000_000,
                read_bytes_remaining: 256 << 20,
                leaves_remaining: 10_000,
            },
            take_state_error: || {
                state
                    .take_read_error()
                    .map(|_| ValueTreeError::Invalid("logistics fixture qualification"))
            },
            cancelled: || false,
        },
        &mut work(),
    )
    .unwrap()
}

#[test]
fn logistics_point_create_delete_receiver_and_typed_transfer_continue_from_roots_only() {
    use LogisticsColumnV1 as C;
    use LogisticsRecordKindV1 as K;
    use LogisticsValueRefV1 as V;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let source_dir = Directory::new();
        let target_dir = Directory::new();
        let target_path = target_dir.0.join("pages");
        let after = {
            let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
            let _caller = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
            let store = IncrementalPageStore::create(
                &source_dir.0.join("pages"),
                memory.clone(),
                compression,
            )
            .unwrap();
            let original = source();
            let roots = seed(&store, &memory, &original);
            let qualification = qualify(&store, &memory, &roots, MemoryClass::Ordinary);
            let parent = parent(roots);
            let live = parent.fence().unwrap();
            let state = store
                .state_session(physical_scope(), MemoryClass::Ordinary)
                .unwrap();
            let values = store.value_session(physical_scope());
            let mut writer = store.value_session(physical_scope());
            let mut transaction = ExecutionTransaction::begin(
                parent.clone(),
                [0x78; 32],
                &state,
                &memory,
                tx_limits(),
            )
            .unwrap();
            {
                let mut read = WorldFieldOverlayContextV1 {
                    transaction: &mut transaction,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 16 << 20,
                    },
                    semantic_bytes: 4096,
                    record_bytes: 8192,
                    take_state_error: || {
                        state
                            .take_read_error()
                            .map(|_| ValueTreeError::Invalid("logistics transaction read"))
                    },
                    cancelled: || false,
                };
                let mut write = MemberRecordWriteContext {
                    sink: &mut writer,
                    memory: &memory,
                    io: &mut logical(),
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 16 << 20,
                    },
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 4096,
                        payload_bytes: 4096,
                        record_bytes: 8192,
                    },
                    cancelled: || false,
                };
                let mut work = work();
                for (kind, id, cell, expected, after) in [
                    (
                        K::Route,
                        " outer/路线 ",
                        LogisticsCellV1::Column(C::Available),
                        Some(V::Bool(false)),
                        Some(V::Bool(true)),
                    ),
                    (
                        K::Path,
                        " outer/path ",
                        LogisticsCellV1::TextItem(LogisticsCollectionV1::RouteIds, 3),
                        None,
                        Some(V::Text("new tail")),
                    ),
                    (
                        K::Path,
                        " outer/path ",
                        LogisticsCellV1::Collection(LogisticsCollectionV1::RouteIds),
                        Some(V::Unsigned(3)),
                        Some(V::Unsigned(4)),
                    ),
                ] {
                    let guard = read.transaction.guard();
                    assert!(
                        change_logistics_cell_overlay_v1(
                            LogisticsCellChangeV1 {
                                key: LogisticsRecordKeyV1 { kind, id },
                                cell,
                                expected,
                                after,
                                max_current_payload_bytes: 128
                            },
                            &mut read,
                            &mut write,
                            &mut work,
                            &guard,
                            &live
                        )
                        .unwrap()
                    );
                }
                let guard = read.transaction.guard();
                create_logistics_record_overlay_v1(
                    LogisticsRecordCreateV1 {
                        key: LogisticsRecordKeyV1 {
                            kind: K::Route,
                            id: " new outer ",
                        },
                        after: LogisticsRecordRefV1::Route(
                            &original.logistics_routes[" outer/路线 "],
                        ),
                        qualification: &qualification,
                    },
                    &mut read,
                    &mut write,
                    &mut work,
                    &guard,
                    &live,
                )
                .unwrap();
                let key = LogisticsRecordKeyV1 {
                    kind: K::Path,
                    id: "",
                };
                let expected = read_logistics_record_root_overlay_v1(key, &mut read).unwrap();
                assert!(expected.is_some());
                let guard = read.transaction.guard();
                assert!(
                    delete_logistics_record_overlay_v1(
                        LogisticsRecordDeleteV1 {
                            key,
                            expected_root: expected,
                            qualification: &qualification
                        },
                        &mut read,
                        &mut write,
                        &mut work,
                        &guard,
                        &live
                    )
                    .unwrap()
                );
                // The second scalar read sees the prior shared-overlay write in this same transaction.
                let got = read_logistics_cell_overlay_v1(
                    LogisticsRecordKeyV1 {
                        kind: K::Route,
                        id: " outer/路线 ",
                    },
                    LogisticsCellV1::Column(C::Available),
                    &mut read,
                    8,
                    &mut std::io::sink(),
                )
                .unwrap();
                assert_eq!(
                    got,
                    LogisticsCellReadV1::Value(LogisticsValueV1::Bool(true))
                );
            }
            let guard = transaction.guard();
            let sealed = transaction.seal(&guard, &live, 1).unwrap();
            assert_eq!(sealed.record.writes.len(), 4);
            let receiver = store.value_session(physical_scope());
            let verified = verify_logistics_transition_v1(
                &parent,
                [0x78; 32],
                &sealed.record,
                &qualification,
                &mut LogisticsTransitionReadContextV1 {
                    values: ValueReadContext {
                        source: &receiver,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 16 << 20,
                    },
                    semantic_bytes: 4096,
                    record_bytes: 8192,
                    max_payload_bytes: 4096,
                    cancelled: || false,
                },
                &mut work(),
            )
            .unwrap();
            assert_eq!(verified.qualification().record_counts(), [2, 1]);
            let after = verified.transition().record.after.clone();
            let cancel = std::sync::atomic::AtomicBool::new(false);
            let mut never = || false;
            let mut io = physical_scope();
            let mut persisted = store
                .buffered_page_writer(&mut io, &cancel, &mut never)
                .unwrap();
            for node in &verified.transition().new_nodes {
                let id = *blake3::hash(&node.bytes).as_bytes();
                assert_eq!(node.digest, blake3::hash(&node.bytes).to_hex().to_string());
                let charge = memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap();
                persisted
                    .put_node(id, charge.retain(node.bytes.clone()))
                    .unwrap();
            }
            persisted.finish().unwrap();
            let target =
                IncrementalPageStore::create(&target_path, memory.clone(), compression).unwrap();
            let state = store
                .state_session(physical_scope(), MemoryClass::Ordinary)
                .unwrap();
            let values = store.value_session(physical_scope());
            let mut io = physical_scope();
            let mut never = || false;
            let mut sink = target
                .buffered_page_writer(&mut io, &cancel, &mut never)
                .unwrap();
            let cancelled_dir = Directory::new();
            let cancelled_store = IncrementalPageStore::create(
                &cancelled_dir.0.join("pages"),
                memory.clone(),
                compression,
            )
            .unwrap();
            let final_phase = std::cell::Cell::new(false);
            let recipe_baseline = std::cell::Cell::new(0);
            let mut after_final_publish = || {
                final_phase.get()
                    && std::fs::read_dir(cancelled_store.root().join("recipes"))
                        .unwrap()
                        .count()
                        > recipe_baseline.get()
            };
            let mut cancelled_io = physical_scope();
            let mut cancelled_sink = cancelled_store
                .buffered_page_writer(&mut cancelled_io, &cancel, &mut after_final_publish)
                .unwrap();
            let transferred = transfer_logistics_family_pages_v1(
                &after,
                &mut LogisticsFamilyReadContextV1 {
                    state: &state,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 16 << 20,
                    },
                    semantic_bytes: 4096,
                    record_bytes: 8192,
                    max_payload_bytes: 4096,
                    outer_scan: &mut StateScanBudget {
                        nodes_remaining: 1_000_000,
                        read_bytes_remaining: 256 << 20,
                        leaves_remaining: 10_000,
                    },
                    take_state_error: || {
                        state
                            .take_read_error()
                            .map(|_| ValueTreeError::Invalid("logistics transfer read"))
                    },
                    cancelled: || false,
                },
                &mut work(),
                10_000,
                &mut flush::MirroredPages {
                    accepted: &mut sink,
                    cancelled: &mut cancelled_sink,
                    memory: &memory,
                },
            )
            .unwrap();
            assert!(transferred.page_count() > 0);
            assert!(sink.pending_reserved_bytes() > 0);
            assert!(cancelled_sink.pending_reserved_bytes() > 0);
            recipe_baseline.set(
                std::fs::read_dir(cancelled_store.root().join("recipes"))
                    .unwrap()
                    .count(),
            );
            final_phase.set(true);
            let rejected_acceptance = cancelled_sink
                .finish()
                .map(|()| transferred.qualification());
            assert!(matches!(
                rejected_acceptance,
                Err(oasis7_distfs::incremental_page_store::PageStoreError::Cancelled)
            ));
            assert!(
                std::fs::read_dir(cancelled_store.root().join("recipes"))
                    .unwrap()
                    .count()
                    > recipe_baseline.get()
            );
            sink.finish().unwrap();
            assert_eq!(transferred.qualification().record_counts(), [2, 1]);
            let target_values = target.value_session(physical_scope());
            transferred
                .visit_pages(|id| -> Result<(), ValueTreeError> {
                    let page = target_values.read_node(id, 8192, &memory, MemoryClass::Ordinary)?;
                    assert_eq!(blake3::hash(&page).as_bytes(), id);
                    Ok(())
                })
                .unwrap();
            after
        };
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let target = IncrementalPageStore::open(
            &target_path,
            memory.clone(),
            compression,
            MemoryClass::Recovery,
        )
        .unwrap();
        let _caller = memory.reserve(16 << 10, MemoryClass::Recovery).unwrap();
        assert_eq!(
            qualify(&target, &memory, &after, MemoryClass::Recovery).record_counts(),
            [2, 1]
        );
        let state = target
            .state_session(physical_scope(), MemoryClass::Recovery)
            .unwrap();
        let values = target.value_session(physical_scope());
        for (kind, id, cell, expected, text) in [
            (
                K::Route,
                " outer/路线 ",
                LogisticsCellV1::Column(C::Available),
                LogisticsValueV1::Bool(true),
                "",
            ),
            (
                K::Route,
                " new outer ",
                LogisticsCellV1::Column(C::RouteId),
                LogisticsValueV1::Text {
                    utf8_bytes: " embedded/路线 ".len() as u64,
                },
                " embedded/路线 ",
            ),
            (
                K::Path,
                " outer/path ",
                LogisticsCellV1::TextItem(LogisticsCollectionV1::RouteIds, 3),
                LogisticsValueV1::Text { utf8_bytes: 8 },
                "new tail",
            ),
        ] {
            let key = LogisticsRecordKeyV1 { kind, id };
            let root = after
                .state
                .iter()
                .find(|r| r.namespace == key.namespace())
                .unwrap();
            let _out = memory.reserve(128, MemoryClass::Recovery).unwrap();
            let mut output = Vec::new();
            let got = read_logistics_cell_v1(
                key,
                cell,
                &mut WorldFieldReadContextV1 {
                    root,
                    state: &state,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class: MemoryClass::Recovery,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 4096,
                    },
                    semantic_bytes: 4096,
                    record_bytes: 8192,
                    take_state_error: || {
                        state
                            .take_read_error()
                            .map(|_| ValueTreeError::Invalid("logistics transferred cold point"))
                    },
                    cancelled: || false,
                },
                128,
                &mut output,
            )
            .unwrap();
            assert_eq!(got, LogisticsCellReadV1::Value(expected));
            assert_eq!(output, text.as_bytes());
        }
    }
}

#[path = "support/incremental_logistics_boundaries.rs"]
mod boundaries;
#[test]
fn logistics_noops_cancel_after_public_resolver_and_poison_real_prefix() {
    boundaries::noops_cancel_after_public_resolver_and_poison_real_prefix();
}

#[test]
fn logistics_state_and_value_carriers_require_caller_pool_and_capacity() {
    boundaries::state_and_value_carriers_require_the_actual_caller_pool_and_capacity();
}

#[path = "support/incremental_logistics_malformed.rs"]
mod malformed;
#[test]
fn logistics_malformed_raw_and_changed_closures_are_rejected() {
    malformed::malformed_raw_and_changed_closures_are_rejected();
}

#[path = "support/incremental_logistics_scaling.rs"]
mod scaling;
#[test]
fn logistics_fixed_scalar_receiver_cost_excludes_unchanged_text_and_vectors() {
    scaling::fixed_scalar_receiver_cost_excludes_unchanged_text_and_vectors();
}

#[path = "support/incremental_logistics_missing.rs"]
mod missing;
#[test]
fn logistics_generic_complete_top_roots_do_not_prove_missing_typed_child() {
    missing::generic_complete_top_roots_do_not_prove_missing_typed_child();
}
#[path = "support/incremental_logistics_put_boundaries.rs"]
mod put_boundaries;
#[test]
fn logistics_final_real_root_put_cancel_or_failure_never_seals_prefix() {
    put_boundaries::final_real_root_put_cancel_or_failure_never_seals_the_successful_prefix();
}

#[path = "support/incremental_logistics_flush.rs"]
mod flush;
#[path = "support/incremental_logistics_oracle.rs"]
mod oracle;

#[path = "support/incremental_material_logistics.rs"]
mod combined;
#[test]
fn material_and_logistics_namespaces_qualify_together_from_same_roots() {
    combined::both_owners_qualify_from_the_same_roots();
}
