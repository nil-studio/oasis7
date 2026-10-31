#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{IncrementalPageStore, PageCompression, PageIoBudget};
use oasis7_formal_protocol::{
    incremental_execution::*, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    MaterialLedgerId, MaterialTransitPriority, WorldState, incremental_field_member::*,
    incremental_field_overlay::*, incremental_field_read::*, incremental_industry_jobs::*,
};
use serde_json::json;
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
    let mut recipe:oasis7_runtime::RecipeJobState=serde_json::from_value(json!({"job_id":17,"requester_agent_id":" agent/界 ","factory_id":"factory","recipe_id":"recipe","accepted_batches":u32::MAX,"consume":[{"kind":"ore","amount":i64::MIN}],"produce":[],"byproducts":[],"power_required":i64::MIN,"duration_ticks":u32::MAX,"consume_ledger":"world","output_ledger":"world","bottleneck_tags":["dup","dup"],"logistics_route_ids":[],"logistics_path_ids":[],"ready_at":u64::MAX})).unwrap();
    recipe.output_ledger = MaterialLedgerId::Factory(" original/厂 ".into());
    let mut transit:oasis7_runtime::MaterialTransitJobState=serde_json::from_value(json!({"job_id":19,"requester_agent_id":"transit","from_ledger":"world","to_ledger":"world","kind":"ore","amount":i64::MAX,"distance_km":i64::MIN,"loss_bps":0,"route_id":"原route","path_id":"path","route_ids":["route","route"],"tariff_electricity_total":-1,"reroute_count":u32::MAX,"ready_at":u64::MAX})).unwrap();
    transit.priority = MaterialTransitPriority::Urgent;
    let completion=serde_json::from_value(json!({"job_id":21,"requester_agent_id":"receipt","factory_id":"different factory","recipe_id":"recipe","accepted_batches":u32::MAX,"produce":[{"kind":"product","amount":i64::MAX}],"byproducts":[],"output_ledger":"world","bottleneck_tags":[],"logistics_route_ids":[],"logistics_path_ids":[]})).unwrap();
    let failure=serde_json::from_value(json!({"action_id":23,"requester_agent_id":"failure","factory_id":"factory","recipe_id":"recipe","blocker_kind":"产品","blocker_detail":" original ","disposition_kind":"lost","consumed_inputs":[{"kind":"ore","amount":i64::MIN}],"lost_inputs":[],"consumed_power":i64::MIN,"lost_power":i64::MAX,"next_action":"recheck","next_recheck":u64::MAX})).unwrap();
    let settlement=serde_json::from_value(json!({"job_id":25,"path_id":"路径","route_ids":["dup","dup"],"tariff_electricity_total":i64::MIN,"owner_payouts":{"":i64::MIN,"原owner":i64::MAX},"governance_tax_electricity":i64::MAX})).unwrap();
    world.pending_recipe_jobs.insert(u64::MAX, recipe);
    world.pending_material_transits.insert(2, transit);
    world.recipe_completion_receipts.insert(3, completion);
    world
        .factory_production_failure_dispositions
        .insert(4, failure);
    world.logistics_settlement_receipts.insert(5, settlement);
    world
}
fn work() -> IndustryWorkBudgetV1 {
    IndustryWorkBudgetV1 {
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
    let count = prepare_industry_jobs_genesis_v1(
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
    let expected = source.pending_recipe_jobs.len()
        + source.pending_material_transits.len()
        + source.recipe_completion_receipts.len()
        + source.factory_production_failure_dispositions.len()
        + source.logistics_settlement_receipts.len();
    assert_eq!(count, expected as u64);
    assert_eq!(rows.len(), 10 + expected);
    let mut roots = RootSetV1::empty("industry-source".into(), [0x41; 32]).unwrap();
    for ns in [StateNamespaceV1::Scheduler, StateNamespaceV1::Authorization] {
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
fn industry_genesis_original_columns_continue_in_small_cold_pools() {
    use IndustryColumnV1 as C;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let dir = Directory::new();
        let path = dir.0.join("pages");
        let roots = {
            let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
            let _source = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
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
            let qualification = qualify_industry_family_v1(
                &roots,
                &mut IndustryFamilyReadContextV1 {
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
                            .map(|_| ValueTreeError::Invalid("industry physical qualification"))
                    },
                    cancelled: || false,
                },
                &mut work(),
            )
            .unwrap();
            assert_eq!(qualification.record_counts(), [1, 1, 1, 1, 1]);
            qualification
                .validate_binding(&roots.digest().unwrap())
                .unwrap();
            oracle::assert_complete_original_rows(&store, &memory, &roots, class);
            for (kind, id, column, expected) in [
                (
                    IndustryRecordKindV1::Recipe,
                    u64::MAX,
                    C::RecordReadyAt,
                    IndustryValueV1::Unsigned(u64::MAX),
                ),
                (
                    IndustryRecordKindV1::Transit,
                    2,
                    C::RecordPriority,
                    IndustryValueV1::Unsigned(1),
                ),
                (
                    IndustryRecordKindV1::Completion,
                    3,
                    C::RecordJobId,
                    IndustryValueV1::Unsigned(21),
                ),
                (
                    IndustryRecordKindV1::Failure,
                    4,
                    C::RecordNextRecheck,
                    IndustryValueV1::Unsigned(u64::MAX),
                ),
                (
                    IndustryRecordKindV1::Settlement,
                    5,
                    C::RecordGovernanceTaxElectricity,
                    IndustryValueV1::Signed(i64::MAX),
                ),
            ] {
                let key = IndustryRecordKeyV1 { kind, id };
                let root = roots
                    .state
                    .iter()
                    .find(|r| r.namespace == key.namespace())
                    .unwrap();
                let mut out = [0; 64];
                let actual = read_industry_cell_v1(
                    key,
                    IndustryCellV1::Column(column),
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
                                .map(|_| ValueTreeError::Invalid("industry physical read"))
                        },
                        cancelled: || false,
                    },
                    64,
                    &mut &mut out[..],
                )
                .unwrap();
                assert_eq!(actual, IndustryCellReadV1::Value(expected));
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
) -> ReservedValue<IndustryFamilyQualificationV1> {
    let state = store.state_session(physical_scope(), class).unwrap();
    let values = store.value_session(physical_scope());
    qualify_industry_family_v1(
        roots,
        &mut IndustryFamilyReadContextV1 {
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
                    .map(|_| ValueTreeError::Invalid("industry fixture qualification"))
            },
            cancelled: || false,
        },
        &mut work(),
    )
    .unwrap()
}
#[test]
fn industry_terminal_delta_receiver_and_buffered_transfer_continue_from_roots_only() {
    use IndustryColumnV1 as C;
    use IndustryRecordKindV1 as K;
    use IndustryValueRefV1 as V;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let dir = Directory::new();
        let target_dir = Directory::new();
        let target_path = target_dir.0.join("pages");
        let after_roots = {
            let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
            let _caller = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
            let store =
                IncrementalPageStore::create(&dir.0.join("pages"), memory.clone(), compression)
                    .unwrap();
            let source = source();
            let mut created_recipe = source.pending_recipe_jobs[&u64::MAX].clone();
            created_recipe.industrial_root_ref = Some(
                serde_json::from_value(json!({
                    "world_binding":" raw/界 ","domain":"d","schema":"s","id":"embedded-id",
                    "revision":u64::MAX,"digest":"Original-Digest"
                }))
                .unwrap(),
            );
            created_recipe.committed_recipe_origin = Some(serde_json::from_value(json!({
                "submission":{"verified_player_id":" player ","public_key":"RawKey",
                "auth_nonce":u64::MAX,"hosted_registration_nonce":" raw nonce ",
                "requester_agent_id":"submit-agent","factory_id":"submit-factory","recipe_id":"submit-recipe"},
                "consensus_action_id":u64::MAX,"consensus_submitter_player_id":"submitter",
                "action_payload_hash":"OriginalHash","committed_height":u64::MAX,"action_root":"OriginalRoot"
            })).unwrap());
            let mut created_completion = source.recipe_completion_receipts[&3].clone();
            created_completion.committed_recipe_origin =
                created_recipe.committed_recipe_origin.clone();
            let roots = seed(&store, &memory, &source);
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
                            .map(|_| ValueTreeError::Invalid("industry fixture state"))
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
                let guard = read.transaction.guard();
                assert!(
                    change_industry_cell_overlay_v1(
                        IndustryCellChangeV1 {
                            key: IndustryRecordKeyV1 {
                                kind: K::Failure,
                                id: 4
                            },
                            cell: IndustryCellV1::Column(C::RecordLostPower),
                            expected: Some(V::Signed(i64::MAX)),
                            after: Some(V::Signed(17)),
                            max_current_payload_bytes: 8
                        },
                        &mut read,
                        &mut write,
                        &mut work,
                        &guard,
                        &live
                    )
                    .unwrap()
                );
                for (kind, id) in [(K::Recipe, u64::MAX), (K::Transit, 2)] {
                    let key = IndustryRecordKeyV1 { kind, id };
                    let expected = read_industry_record_root_overlay_v1(key, &mut read).unwrap();
                    assert!(expected.is_some());
                    let guard = read.transaction.guard();
                    assert!(
                        delete_industry_record_overlay_v1(
                            IndustryRecordDeleteV1 {
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
                }
                for (kind, id, after) in [
                    (K::Recipe, 60, IndustryRecordRefV1::Recipe(&created_recipe)),
                    (
                        K::Transit,
                        20,
                        IndustryRecordRefV1::Transit(&source.pending_material_transits[&2]),
                    ),
                    (
                        K::Completion,
                        30,
                        IndustryRecordRefV1::Completion(&created_completion),
                    ),
                    (
                        K::Settlement,
                        40,
                        IndustryRecordRefV1::Settlement(&source.logistics_settlement_receipts[&5]),
                    ),
                ] {
                    let guard = read.transaction.guard();
                    create_industry_record_overlay_v1(
                        IndustryRecordCreateV1 {
                            key: IndustryRecordKeyV1 { kind, id },
                            after,
                            qualification: &qualification,
                        },
                        &mut read,
                        &mut write,
                        &mut work,
                        &guard,
                        &live,
                    )
                    .unwrap();
                }
            }
            let guard = transaction.guard();
            let sealed = transaction.seal(&guard, &live, 1).unwrap();
            assert_eq!(sealed.record.writes.len(), 7);
            let receiver = store.value_session(physical_scope());
            let verified = verify_industry_transition_v1(
                &parent,
                [0x78; 32],
                &sealed.record,
                &qualification,
                &mut IndustryTransitionReadContextV1 {
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
            assert_eq!(verified.qualification().record_counts(), [1, 1, 2, 1, 2]);
            let after = verified.transition().record.after.clone();
            let cancel = std::sync::atomic::AtomicBool::new(false);
            let mut io = physical_scope();
            let mut never = || false;
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
            let mut target_io = physical_scope();
            let mut never = || false;
            let mut sink = target
                .buffered_page_writer(&mut target_io, &cancel, &mut never)
                .unwrap();
            let transfer_state = store
                .state_session(physical_scope(), MemoryClass::Ordinary)
                .unwrap();
            let transfer_values = store.value_session(physical_scope());
            let transferred = transfer_industry_family_pages_v1(
                &after,
                &mut IndustryFamilyReadContextV1 {
                    state: &transfer_state,
                    values: ValueReadContext {
                        source: &transfer_values,
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
                        transfer_state
                            .take_read_error()
                            .map(|_| ValueTreeError::Invalid("industry transfer source"))
                    },
                    cancelled: || false,
                },
                &mut work(),
                // Seven complete records (including two newly created pending jobs).
                40_000,
                &mut flush::MirroredPages {
                    accepted: &mut sink,
                    cancelled: &mut cancelled_sink,
                    memory: &memory,
                },
            )
            .unwrap();
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
            assert_eq!(transferred.qualification().record_counts(), [1, 1, 2, 1, 2]);
            assert!(transferred.page_count() > 0);
            eprintln!(
                "industry complete transfer codec={compression:?} pages={}",
                transferred.page_count()
            );
            after
        };
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let store = IncrementalPageStore::open(
            &target_path,
            memory.clone(),
            compression,
            MemoryClass::Recovery,
        )
        .unwrap();
        let qualification = qualify(&store, &memory, &after_roots, MemoryClass::Recovery);
        assert_eq!(qualification.record_counts(), [1, 1, 2, 1, 2]);
    }
}

#[path = "support/incremental_industry_oracle.rs"]
mod oracle;

#[path = "support/incremental_industry_boundaries.rs"]
mod boundaries;
#[test]
fn industry_noops_cancel_after_public_resolver_and_poison_real_prefix() {
    boundaries::noops_cancel_after_public_resolver_and_poison_real_prefix();
}
#[test]
fn industry_state_and_value_carriers_require_caller_pool_and_capacity() {
    boundaries::state_and_value_carriers_require_the_actual_caller_pool_and_capacity();
}

#[path = "support/incremental_industry_malformed.rs"]
mod malformed;
#[test]
fn industry_malformed_raw_and_changed_closures_are_rejected() {
    malformed::malformed_raw_and_changed_closures_are_rejected();
}

#[path = "support/incremental_industry_scaling.rs"]
mod scaling;
#[test]
fn industry_fixed_scalar_receiver_cost_excludes_unchanged_text_and_vectors() {
    scaling::fixed_scalar_receiver_cost_excludes_unchanged_text_and_vectors();
}

#[path = "support/incremental_industry_missing.rs"]
mod missing;
#[test]
fn industry_generic_complete_top_roots_do_not_prove_missing_typed_child() {
    missing::generic_complete_top_roots_do_not_prove_missing_typed_child();
}

#[path = "support/incremental_industry_put_boundaries.rs"]
mod put_boundaries;
#[test]
fn industry_final_real_root_put_cancel_or_failure_never_seals_prefix() {
    put_boundaries::final_real_root_put_cancel_or_failure_never_seals_the_successful_prefix();
}

#[path = "support/incremental_industry_flush.rs"]
mod flush;
