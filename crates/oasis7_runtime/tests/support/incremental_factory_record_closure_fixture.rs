#![cfg(all(unix, not(target_arch = "wasm32")))]

use oasis7_distfs::incremental_page_store::{IncrementalPageStore, PageIoBudget};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionWriteV1, RootSetV1},
    incremental_state::{
        StateDeltaV1, StateNamespaceV1, StatePageWriter, StateRootV1, StateScanBudget,
        StateScanContext, StateWriteV1,
    },
    incremental_value::{ValueIoBudget, ValueNodeWriter, ValueReadContext, ValueStreamWriter},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    FactoryState, WorldState,
    incremental_factory_records::{FactoryWorkBudgetV1, prepare_factory_records_genesis_v1},
    incremental_field_member::{MemberRecordLimitsV1, MemberRecordWriteContext, SemanticKeyBudget},
};

// One authenticated traversal feeds two provisional destinations. Each copied
// carrier is admitted before allocation; neither sink result grants authority.
struct MirroredPages<'a, A, B> {
    accepted: &'a mut A,
    cancelled: &'a mut B,
    memory: &'a MemoryBudget,
}
impl<A: ValueNodeWriter, B: ValueNodeWriter> ValueNodeWriter for MirroredPages<'_, A, B> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), oasis7_formal_protocol::incremental_value::ValueTreeError> {
        let charge = self.memory.reserve(bytes.len(), MemoryClass::Ordinary)?;
        let copy = charge.retain(bytes.to_vec());
        self.accepted.put_node(id, bytes)?;
        self.cancelled.put_node(id, copy)
    }
}

pub(crate) struct Directory(pub(crate) std::path::PathBuf);
impl Directory {
    pub(crate) fn new(label: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "oasis7-factory-record-closure-{}-{nonce}-{sequence}-{label}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

pub(crate) fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 1_000_000,
        read_bytes_remaining: 512 << 20,
        writes_remaining: 1_000_000,
        write_bytes_remaining: 512 << 20,
    }
}

pub(crate) fn logical() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 512 << 20,
        write_bytes_remaining: 512 << 20,
    }
}

fn factory(embedded_id: &str, display_name: String) -> FactoryState {
    serde_json::from_value(serde_json::json!({
        "factory_id": embedded_id,
        "site_id": "site",
        "builder_agent_id": "builder",
        "built_at": 17,
        "spec": {
            "factory_id": "spec-id",
            "display_name": display_name,
            "tier": 3,
            "tags": ["", "repeat", "repeat"],
            "build_cost": [],
            "build_time_ticks": 11,
            "base_power_draw": -7,
            "recipe_slots": 2,
            "throughput_bps": 123,
            "maintenance_per_tick": 5
        }
    }))
    .unwrap()
}

pub(crate) fn source() -> WorldState {
    let large_name = format!("nested:界/{}:end", "界".repeat(140_000));
    WorldState {
        factories: [
            (
                "outer:small/Δ".into(),
                factory("embedded-small-id", "small\0name/Δ".into()),
            ),
            (
                "outer:large/Δ".into(),
                factory("embedded-large-id", large_name),
            ),
        ]
        .into(),
        ..WorldState::default()
    }
}

pub(crate) fn seed_factory_roots(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    source: &WorldState,
) -> RootSetV1 {
    let mut values = store.value_session(physical());
    let _rows_charge = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let mut rows: Vec<ReservedValue<ExecutionWriteV1>> = Vec::with_capacity(1024);
    let mut records_remaining = 100_000;
    let mut work = FactoryWorkBudgetV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 16 << 20,
        scan: StateScanBudget {
            nodes_remaining: 1_000_000,
            read_bytes_remaining: 512 << 20,
            leaves_remaining: 100_000,
        },
    };
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 512 << 20,
    };
    let mut io = io_budget();
    let emitted = prepare_factory_records_genesis_v1(
        source,
        &mut MemberRecordWriteContext {
            sink: &mut values,
            memory,
            io: &mut io,
            key_work: &mut key_work,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 8 << 20,
                payload_bytes: 8 << 20,
                record_bytes: 16 << 20,
            },
            cancelled: || false,
        },
        &mut records_remaining,
        &mut work,
        &mut |row| {
            assert!(rows.len() < 1024);
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(
        emitted,
        source.factories.len() as u64 + source.pending_factory_builds.len() as u64
    );

    let mut roots = RootSetV1::empty("factory-child-closure".into(), [0x52; 32]).unwrap();
    for namespace in [StateNamespaceV1::Business, StateNamespaceV1::Scheduler] {
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let mut writes: Vec<_> = rows
            .iter()
            .filter(|row| row.namespace == namespace)
            .map(|row| StateWriteV1 {
                key: row.mutation.key.clone(),
                before: None,
                after: row.mutation.after,
            })
            .collect();
        writes.sort_by(|left, right| left.key.cmp(&right.key));
        assert!(writes.windows(2).all(|pair| pair[0].key < pair[1].key));
        let workspace = memory.reserve(16 << 20, MemoryClass::Ordinary).unwrap();
        let delta = workspace.retain(
            StateDeltaV1::build(
                StateRootV1::empty(namespace).unwrap(),
                writes,
                &state,
                16 << 20,
            )
            .unwrap(),
        );
        values.put_state_pages(&delta, memory).unwrap();
        let root_index = roots
            .state
            .iter()
            .position(|root| root.namespace == namespace)
            .unwrap();
        roots.state[root_index] = delta.after.clone();
    }
    roots
}

fn io_budget() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 512 << 20,
        write_bytes_remaining: 512 << 20,
    }
}

/// Store only the six generic roots. RecordRootRef values are generic value
/// closures, so this deliberately does not follow their typed child StateRoot
/// edges. This fixture models a generic-only transfer, not a complete transfer.
pub(crate) fn copy_generic_roots(
    source_store: &IncrementalPageStore,
    target_store: &IncrementalPageStore,
    roots: &RootSetV1,
    source_memory: &MemoryBudget,
    target_memory: &MemoryBudget,
    class: MemoryClass,
) -> RootSetV1 {
    let mut target_roots = roots.clone();
    for (index, source_root) in roots.state.iter().enumerate() {
        if source_root.leaf_count == 0 {
            continue;
        }
        let source_state = source_store.state_session(physical(), class).unwrap();
        let target_state = target_store.state_session(physical(), class).unwrap();
        let source_values = source_store.value_session(physical());
        let mut target_values = target_store.value_session(physical());
        let mut source_value_io = io_budget();
        let mut target_value_io = io_budget();
        let _batch = target_memory.reserve(1 << 20, class).unwrap();
        let mut writes = Vec::with_capacity(source_root.leaf_count as usize);
        let mut scan_io = StateScanBudget {
            nodes_remaining: 1_000_000,
            read_bytes_remaining: 512 << 20,
            leaves_remaining: source_root.leaf_count,
        };
        let scanned = StateScanContext {
            source: &source_state,
            memory: source_memory,
            class,
            io: &mut scan_io,
        }
        .visit_entries(
            source_root,
            |entry| {
                let mut source_context = ValueReadContext {
                    source: &source_values,
                    memory: source_memory,
                    class,
                    io: &mut source_value_io,
                };
                let closure = source_context
                    .verify_closure(&entry.value, 512 << 20, || false)
                    .map_err(|_| "source generic value closure")?;
                assert_eq!(closure.root().digest().unwrap(), entry.value);
                let root = source_context
                    .load_root(&entry.value)
                    .map_err(|_| "source generic value root")?;
                let root_bytes = root.canonical_bytes().unwrap();
                let length_offset = b"oasis7.iws.value-root.v1\0".len() + 4 + 32;
                let logical_len = u64::from_be_bytes(
                    root_bytes[length_offset..length_offset + 8]
                        .try_into()
                        .unwrap(),
                );
                let mut output = ValueStreamWriter::new(
                    &mut target_values,
                    target_memory,
                    &mut target_value_io,
                    logical_len,
                    || false,
                )
                .map_err(|_| "target generic value stream")?;
                let copied = source_context
                    .read_range(&root, 0, logical_len, &mut output, || false)
                    .map_err(|_| "source generic value bytes")?;
                assert_eq!(copied, logical_len);
                let copied_root = output.finish().map_err(|_| "target generic value finish")?;
                assert_eq!(copied_root.digest().unwrap(), entry.value);
                writes.push(StateWriteV1 {
                    key: entry.key.clone(),
                    before: None,
                    after: Some(entry.value),
                });
                Ok::<_, &'static str>(())
            },
            || false,
        )
        .unwrap_or_else(|error| panic!("generic root scan/copy failed: {error:?}"));
        assert_eq!(scanned, source_root.leaf_count);
        writes.sort_by(|left, right| left.key.cmp(&right.key));
        assert!(writes.windows(2).all(|pair| pair[0].key < pair[1].key));
        let workspace = target_memory.reserve(8 << 20, class).unwrap();
        let delta = workspace.retain(
            StateDeltaV1::build(
                StateRootV1::empty(source_root.namespace).unwrap(),
                writes,
                &target_state,
                8 << 20,
            )
            .unwrap(),
        );
        assert_eq!(delta.after, *source_root);
        target_values
            .put_state_pages(&delta, target_memory)
            .unwrap();
        target_roots.state[index] = delta.after.clone();
    }
    target_roots
}

/// Full generic top-state scan and every leaf's complete ValueRoot closure.
/// This deliberately does not inspect or certify RecordRootRef child StateRoots.
pub(crate) fn verify_generic_root_values(
    store: &IncrementalPageStore,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    class: MemoryClass,
) {
    for root in &roots.state {
        let state = store.state_session(physical(), class).unwrap();
        let values = store.value_session(physical());
        let mut scan_io = StateScanBudget {
            nodes_remaining: 1_000_000,
            read_bytes_remaining: 512 << 20,
            leaves_remaining: root.leaf_count,
        };
        let mut value_io = io_budget();
        let scanned = StateScanContext {
            source: &state,
            memory,
            class,
            io: &mut scan_io,
        }
        .visit_entries(
            root,
            |entry| {
                ValueReadContext {
                    source: &values,
                    memory,
                    class,
                    io: &mut value_io,
                }
                .verify_closure(&entry.value, 512 << 20, || false)
                .map_err(|_| "generic value closure")?;
                Ok::<_, &'static str>(())
            },
            || false,
        )
        .unwrap_or_else(|error| panic!("generic root closure failed: {error:?}"));
        assert_eq!(scanned, root.leaf_count);
    }
}

pub(crate) fn qualified_factory_create_receiver_and_typed_transfer_preserve_cold_child() {
    use super::{
        assert_literal_child_value, execution_parent, factory_work, limits, logical, physical,
        qualify_factory_roots, read_display_name, transaction_limits, typed,
    };
    use FactoryColumnV1 as C;
    use FactoryValueRefV1 as V;
    use oasis7_distfs::incremental_page_store::PageCompression;
    use oasis7_formal_protocol::incremental_execution::ExecutionTransaction;
    use oasis7_runtime::{
        incremental_factory_records::{
            FactoryCellChangeV1, FactoryCellReadV1, FactoryCellV1, FactoryColumnV1,
            FactoryFamilyReadContextV1, FactoryRecordCreateV1, FactoryRecordKeyV1,
            FactoryRecordRefV1, FactoryTransitionReadContextV1, FactoryValueRefV1, FactoryValueV1,
            change_factory_cell_overlay_v1, create_factory_record_overlay_v1,
            mark_factory_family_pages_v1, read_factory_cell_v1, transfer_factory_family_pages_v1,
            verify_factory_transition_v1,
        },
        incremental_field_member::{MemberRecordWriteContext, SemanticKeyBudget},
        incremental_field_overlay::WorldFieldOverlayContextV1,
        incremental_field_read::WorldFieldReadContextV1,
    };

    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let source = source();
        let large_name = source.factories["outer:large/Δ"].spec.display_name.clone();
        let new_factory = source.factories["outer:small/Δ"].clone();
        let new_display = new_factory.spec.display_name.clone();
        let embedded_id = new_factory.factory_id.clone();
        let source_dir = Directory::new("qualified-source");
        let source_path = source_dir.0.join("pages");
        let target_dir = Directory::new("typed-transfer-target");
        let target_path = target_dir.0.join("pages");

        let committed_roots = {
            let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
            let _caller = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
            let store =
                IncrementalPageStore::create(&source_path, memory.clone(), compression).unwrap();
            let parent_roots = seed_factory_roots(&store, &memory, &source);
            let qualification =
                qualify_factory_roots(&store, &memory, &parent_roots, MemoryClass::Ordinary);
            assert_eq!(qualification.record_counts(), [2, 0]);
            qualification
                .validate_binding(&parent_roots.digest().unwrap())
                .unwrap();

            // Establish the read cost of the exact untouched large field. The
            // receiver below gets less than its payload length in logical read
            // allowance and must still accept the small scalar/new-record delta.
            let (large_reads, large_bytes, large_logical_bytes) = read_display_name(
                &store,
                &memory,
                &parent_roots,
                "outer:large/Δ",
                &large_name,
                MemoryClass::Ordinary,
            );
            assert!(large_reads > 0 && large_bytes > 0);
            assert!(large_logical_bytes >= large_name.len() as u64);

            let parent = execution_parent(parent_roots.clone());
            let live = parent.fence().unwrap();
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let mut transaction = ExecutionTransaction::begin(
                parent.clone(),
                [0x78; 32],
                &state,
                &memory,
                transaction_limits(),
            )
            .unwrap();
            let read_values = store.value_session(physical());
            let mut write_values = store.value_session(physical());
            let mut overlay_io = logical();
            let mut write_io = logical();
            let mut overlay_key_work = SemanticKeyBudget {
                bytes_remaining: 64 << 20,
            };
            let mut write_key_work = SemanticKeyBudget {
                bytes_remaining: 64 << 20,
            };
            let mut work = factory_work();

            // A scalar update to the large record exercises the receiver's
            // unchanged-payload path; the new outer key deliberately differs
            // from the embedded factory_id carried by the source record.
            {
                let mut read = WorldFieldOverlayContextV1 {
                    transaction: &mut transaction,
                    values: ValueReadContext {
                        source: &read_values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut overlay_io,
                    },
                    key_work: &mut overlay_key_work,
                    semantic_bytes: 8 << 20,
                    record_bytes: 16 << 20,
                    take_state_error: || state.take_read_error().map(typed),
                    cancelled: || false,
                };
                let mut write = MemberRecordWriteContext {
                    sink: &mut write_values,
                    memory: &memory,
                    io: &mut write_io,
                    key_work: &mut write_key_work,
                    limits: limits(),
                    cancelled: || false,
                };
                let guard = read.transaction.guard();
                assert!(
                    change_factory_cell_overlay_v1(
                        FactoryCellChangeV1 {
                            key: FactoryRecordKeyV1::Factory("outer:large/Δ"),
                            cell: FactoryCellV1::Column(C::FactoryBuiltAt),
                            expected: Some(V::Unsigned(17)),
                            after: Some(V::Unsigned(18)),
                            max_current_payload_bytes: 1024,
                        },
                        &mut read,
                        &mut write,
                        &mut work,
                        &guard,
                        &live,
                    )
                    .unwrap()
                );
            }
            {
                let mut read = WorldFieldOverlayContextV1 {
                    transaction: &mut transaction,
                    values: ValueReadContext {
                        source: &read_values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut overlay_io,
                    },
                    key_work: &mut overlay_key_work,
                    semantic_bytes: 8 << 20,
                    record_bytes: 16 << 20,
                    take_state_error: || state.take_read_error().map(typed),
                    cancelled: || false,
                };
                let mut write = MemberRecordWriteContext {
                    sink: &mut write_values,
                    memory: &memory,
                    io: &mut write_io,
                    key_work: &mut write_key_work,
                    limits: limits(),
                    cancelled: || false,
                };
                let guard = read.transaction.guard();
                assert!(
                    create_factory_record_overlay_v1(
                        FactoryRecordCreateV1 {
                            key: FactoryRecordKeyV1::Factory("outer:new/λ"),
                            after: FactoryRecordRefV1::Factory(&new_factory),
                            qualification: &qualification,
                        },
                        &mut read,
                        &mut write,
                        &mut work,
                        &guard,
                        &live,
                    )
                    .unwrap()
                );
            }

            let guard = transaction.guard();
            let sealed = transaction.seal(&guard, &live, 1).unwrap();
            assert_eq!(sealed.record.writes.len(), 2);

            let receiver_values = store.value_session(physical());
            // This combined transaction includes a complete new record. Its
            // explicit new-child scan is measured separately from point scaling.
            let mut receiver_io = logical();
            let initial_receiver_reads = receiver_io.nodes_remaining;
            let initial_receiver_bytes = receiver_io.read_bytes_remaining;
            let mut receiver_key_work = SemanticKeyBudget {
                bytes_remaining: 64 << 20,
            };
            let mut receiver_work = factory_work();
            let verified = verify_factory_transition_v1(
                &parent,
                [0x78; 32],
                &sealed.record,
                &qualification,
                &mut FactoryTransitionReadContextV1 {
                    values: ValueReadContext {
                        source: &receiver_values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut receiver_io,
                    },
                    key_work: &mut receiver_key_work,
                    semantic_bytes: 8 << 20,
                    record_bytes: 16 << 20,
                    max_payload_bytes: 8 << 20,
                    cancelled: || false,
                },
                &mut receiver_work,
            )
            .unwrap();
            let receiver_reads = initial_receiver_reads - receiver_io.nodes_remaining;
            let receiver_bytes = initial_receiver_bytes - receiver_io.read_bytes_remaining;
            assert!(receiver_reads > 0);
            eprintln!(
                "combined scalar+create receiver reads={receiver_reads} bytes={receiver_bytes}; untouched_text_logical={large_logical_bytes}"
            );
            assert_eq!(
                verified.qualification().record_counts(),
                [3, 0],
                "the receiver adds one Factory while preserving the pending-build count"
            );
            assert_eq!(
                verified.transition().record.after,
                sealed.record.after,
                "the typed receiver must preserve the verified generic transition"
            );
            let after_roots = verified.transition().record.after.clone();

            // Install only the independently verified transition state nodes;
            // the record-local StateRoot and values were emitted provisionally
            // by the same transaction into this source store.
            let mut persist = store.value_session(physical());
            for node in &verified.transition().new_nodes {
                let digest = *blake3::hash(&node.bytes).as_bytes();
                assert_eq!(node.digest, blake3::hash(&node.bytes).to_hex().to_string());
                persist
                    .put_node(
                        digest,
                        memory
                            .reserve(node.bytes.len(), MemoryClass::Ordinary)
                            .unwrap()
                            .retain(node.bytes.clone()),
                    )
                    .unwrap();
            }

            let child = assert_literal_child_value(
                &store,
                &memory,
                &after_roots,
                "outer:new/λ",
                &new_display,
            );
            let child_page_id = blake3::Hash::from_hex(&child.node_root).unwrap();

            let mark_state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let mark_values = store.value_session(physical());
            let mut mark_io = logical();
            let mut mark_key_work = SemanticKeyBudget {
                bytes_remaining: 64 << 20,
            };
            let mut mark_scan = StateScanBudget {
                nodes_remaining: 1_000_000,
                read_bytes_remaining: 512 << 20,
                leaves_remaining: 100_000,
            };
            let marked = mark_factory_family_pages_v1(
                &after_roots,
                &mut FactoryFamilyReadContextV1 {
                    state: &mark_state,
                    values: ValueReadContext {
                        source: &mark_values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut mark_io,
                    },
                    key_work: &mut mark_key_work,
                    semantic_bytes: 8 << 20,
                    record_bytes: 16 << 20,
                    max_payload_bytes: 8 << 20,
                    outer_scan: &mut mark_scan,
                    take_state_error: || mark_state.take_read_error().map(typed),
                    cancelled: || false,
                },
                &mut factory_work(),
                20_000,
            )
            .unwrap();
            assert!(marked.contains(child_page_id.as_bytes()));
            assert_eq!(marked.qualification().record_counts(), [3, 0]);

            let target_store =
                IncrementalPageStore::create(&target_path, memory.clone(), compression).unwrap();
            let cancelled_dir = Directory::new("typed-transfer-final-cancel");
            let cancelled_store = IncrementalPageStore::create(
                &cancelled_dir.0.join("pages"),
                memory.clone(),
                compression,
            )
            .unwrap();
            let cancellation = std::sync::atomic::AtomicBool::new(false);
            let mut target_io = physical();
            let mut no_cancel = || false;
            let mut target_values = target_store
                .buffered_page_writer(&mut target_io, &cancellation, &mut no_cancel)
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
            let mut cancelled_io = physical();
            let mut cancelled_values = cancelled_store
                .buffered_page_writer(&mut cancelled_io, &cancellation, &mut after_final_publish)
                .unwrap();
            let transfer_state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let transfer_values = store.value_session(physical());
            let mut transfer_io = logical();
            let mut transfer_key_work = SemanticKeyBudget {
                bytes_remaining: 64 << 20,
            };
            let mut transfer_scan = StateScanBudget {
                nodes_remaining: 1_000_000,
                read_bytes_remaining: 512 << 20,
                leaves_remaining: 100_000,
            };
            let transferred = transfer_factory_family_pages_v1(
                &after_roots,
                &mut FactoryFamilyReadContextV1 {
                    state: &transfer_state,
                    values: ValueReadContext {
                        source: &transfer_values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut transfer_io,
                    },
                    key_work: &mut transfer_key_work,
                    semantic_bytes: 8 << 20,
                    record_bytes: 16 << 20,
                    max_payload_bytes: 8 << 20,
                    outer_scan: &mut transfer_scan,
                    take_state_error: || transfer_state.take_read_error().map(typed),
                    cancelled: || false,
                },
                &mut factory_work(),
                20_000,
                &mut MirroredPages {
                    accepted: &mut target_values,
                    cancelled: &mut cancelled_values,
                    memory: &memory,
                },
            )
            .unwrap();
            assert!(target_values.pending_reserved_bytes() > 0);
            assert!(cancelled_values.pending_reserved_bytes() > 0);
            recipe_baseline.set(
                std::fs::read_dir(cancelled_store.root().join("recipes"))
                    .unwrap()
                    .count(),
            );
            final_phase.set(true);
            // The provisional traversal receipt is usable only after sink finish.
            let rejected_acceptance = cancelled_values
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
            target_values.finish().unwrap();
            assert!(transferred.contains(child_page_id.as_bytes()));
            assert_eq!(transferred.qualification().record_counts(), [3, 0]);
            assert_eq!(transferred.page_count(), marked.page_count());

            let _marked_ids_charge = memory
                .reserve(marked.page_count() * 32, MemoryClass::Ordinary)
                .unwrap();
            let mut marked_ids = Vec::with_capacity(marked.page_count());
            marked
                .visit_pages(|id| {
                    marked_ids.push(*id);
                    Ok::<_, ()>(())
                })
                .unwrap();
            let _transferred_ids_charge = memory
                .reserve(transferred.page_count() * 32, MemoryClass::Ordinary)
                .unwrap();
            let mut transferred_ids = Vec::with_capacity(transferred.page_count());
            transferred
                .visit_pages(|id| {
                    transferred_ids.push(*id);
                    Ok::<_, ()>(())
                })
                .unwrap();
            assert_eq!(marked_ids, transferred_ids);
            after_roots
        };

        // The only retained source descriptor is the RootSet. After the source
        // store and memory pool close, the destination has every typed child
        // page needed to qualify and continue point reads from a new pool.
        let cold_memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let _cold_caller = cold_memory
            .reserve(128 << 10, MemoryClass::Recovery)
            .unwrap();
        let cold_store = IncrementalPageStore::open(
            &target_path,
            cold_memory.clone(),
            compression,
            MemoryClass::Recovery,
        )
        .unwrap();
        let cold_qualification = qualify_factory_roots(
            &cold_store,
            &cold_memory,
            &committed_roots,
            MemoryClass::Recovery,
        );
        assert_eq!(cold_qualification.record_counts(), [3, 0]);
        let (reopened_reads, reopened_bytes, _logical_bytes) = read_display_name(
            &cold_store,
            &cold_memory,
            &committed_roots,
            "outer:new/λ",
            &new_display,
            MemoryClass::Recovery,
        );
        assert!(reopened_reads > 0 && reopened_bytes > 0);

        let state = cold_store
            .state_session(physical(), MemoryClass::Recovery)
            .unwrap();
        let values = cold_store.value_session(physical());
        let _id_output_charge = cold_memory
            .reserve(embedded_id.len().max(1), MemoryClass::Recovery)
            .unwrap();
        let mut id_output = vec![0; embedded_id.len()];
        let mut key_work = SemanticKeyBudget {
            bytes_remaining: 4096,
        };
        let result = read_factory_cell_v1(
            FactoryRecordKeyV1::Factory("outer:new/λ"),
            FactoryCellV1::Column(C::FactoryFactoryId),
            &mut WorldFieldReadContextV1 {
                root: &committed_roots.state[0],
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &cold_memory,
                    class: MemoryClass::Recovery,
                    io: &mut logical(),
                },
                key_work: &mut key_work,
                semantic_bytes: 1024,
                record_bytes: 8192,
                take_state_error: || state.take_read_error().map(typed),
                cancelled: || false,
            },
            1024,
            &mut &mut id_output[..],
        )
        .unwrap();
        assert_eq!(
            result,
            FactoryCellReadV1::Value(FactoryValueV1::Text {
                utf8_bytes: embedded_id.len() as u64,
            })
        );
        assert_eq!(id_output, embedded_id.as_bytes());
        assert_ne!("outer:new/λ", embedded_id);
    }
}

/// Isolate ordinary scalar acceptance from the deliberately full new-record
/// scan in the create/transfer scenario. The public provider counts real reads.
pub(crate) fn scalar_receiver_cost_does_not_follow_unchanged_text_and_vector_size() {
    use super::{
        execution_parent, factory_work, limits, qualify_factory_roots, transaction_limits, typed,
    };
    use oasis7_formal_protocol::incremental_execution::ExecutionTransaction;
    use oasis7_formal_protocol::incremental_value::ValueNodeReader;
    use oasis7_runtime::{incremental_factory_records::*, incremental_field_overlay::*};
    use std::{cell::Cell, sync::Arc};
    struct Count<'a, R> {
        source: &'a R,
        state: Cell<(u64, u64)>,
        value: Cell<(u64, u64)>,
    }
    impl<R: ValueNodeReader> ValueNodeReader for Count<'_, R> {
        fn read_node(
            &self,
            id: &[u8; 32],
            max: usize,
            memory: &MemoryBudget,
            class: MemoryClass,
        ) -> Result<
            Arc<ReservedValue<Vec<u8>>>,
            oasis7_formal_protocol::incremental_value::ValueTreeError,
        > {
            let page = self.source.read_node(id, max, memory, class)?;
            let counter = if page.starts_with(b"oasis7.iws.state-page.v1\0") {
                &self.state
            } else {
                &self.value
            };
            let (nodes, bytes) = counter.get();
            counter.set((nodes + 1, bytes + page.len() as u64));
            Ok(page)
        }
    }
    for compression in [
        oasis7_distfs::incremental_page_store::PageCompression::Raw,
        oasis7_distfs::incremental_page_store::PageCompression::Zstd,
    ] {
        let mut samples = Vec::with_capacity(2);
        for (bytes, tags) in [(8_000, 4), (420_000, 64)] {
            let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
            let _caller = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
            let directory = Directory::new("scalar-receiver-count");
            let store = IncrementalPageStore::create(
                &directory.0.join("pages"),
                memory.clone(),
                compression,
            )
            .unwrap();
            let mut source = source();
            source.factories.remove("outer:large/Δ");
            let factory = source.factories.get_mut("outer:small/Δ").unwrap();
            factory.spec.display_name = "x".repeat(bytes);
            factory.spec.tags = (0..tags).map(|i| format!("tag{i}")).collect();
            let roots = seed_factory_roots(&store, &memory, &source);
            let qualification =
                qualify_factory_roots(&store, &memory, &roots, MemoryClass::Ordinary);
            let parent = execution_parent(roots);
            let live = parent.fence().unwrap();
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let values = store.value_session(physical());
            let mut writer = store.value_session(physical());
            let mut tx = ExecutionTransaction::begin(
                parent.clone(),
                [0x7b; 32],
                &state,
                &memory,
                transaction_limits(),
            )
            .unwrap();
            let mut work = factory_work();
            {
                let mut read = WorldFieldOverlayContextV1 {
                    transaction: &mut tx,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 1 << 20,
                    },
                    semantic_bytes: 8 << 20,
                    record_bytes: 16 << 20,
                    take_state_error: || state.take_read_error().map(typed),
                    cancelled: || false,
                };
                let mut write = MemberRecordWriteContext {
                    sink: &mut writer,
                    memory: &memory,
                    io: &mut logical(),
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 1 << 20,
                    },
                    limits: limits(),
                    cancelled: || false,
                };
                let guard = read.transaction.guard();
                assert!(
                    change_factory_cell_overlay_v1(
                        FactoryCellChangeV1 {
                            key: FactoryRecordKeyV1::Factory("outer:small/Δ"),
                            cell: FactoryCellV1::Column(FactoryColumnV1::FactoryBuiltAt),
                            expected: Some(FactoryValueRefV1::Unsigned(17)),
                            after: Some(FactoryValueRefV1::Unsigned(18)),
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
            }
            let guard = tx.guard();
            let sealed = tx.seal(&guard, &live, 1).unwrap();
            assert_eq!(sealed.record.writes.len(), 1);
            let counting = Count {
                source: &values,
                state: Cell::new((0, 0)),
                value: Cell::new((0, 0)),
            };
            let mut io = logical();
            let initial = (io.nodes_remaining, io.read_bytes_remaining);
            let verified = verify_factory_transition_v1(
                &parent,
                [0x7b; 32],
                &sealed.record,
                &qualification,
                &mut FactoryTransitionReadContextV1 {
                    values: ValueReadContext {
                        source: &counting,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut io,
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 1 << 20,
                    },
                    semantic_bytes: 8 << 20,
                    record_bytes: 16 << 20,
                    max_payload_bytes: 8 << 20,
                    cancelled: || false,
                },
                &mut factory_work(),
            )
            .unwrap();
            assert_eq!(verified.qualification().record_counts(), [1, 0]);
            let state = counting.state.get();
            let value = counting.value.get();
            let used = (
                initial.0 - io.nodes_remaining,
                initial.1 - io.read_bytes_remaining,
            );
            eprintln!(
                "receiver scalar S_text={bytes} S_tags={tags} state={state:?} value={value:?} logical={used:?}"
            );
            assert!(state.0 > 0 && value.0 > 0);
            samples.push((state, value, used));
        }
        let small = samples[0];
        let large = samples[1];
        assert!(
            large.2.0 <= small.2.0 + 16,
            "unchanged collection/text cannot add a field traversal: {samples:?}"
        );
        assert!(
            large.2.1 <= small.2.1 + (64 << 10),
            "unchanged payload size cannot expand scalar receiver reads: {samples:?}"
        );
    }
}
