//! The same real module bytes pass the original hash and sequential P writer.
//! The original and Wasm Catalog outputs are DATA, never FactSet or acceptance authority.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        FormalError,
        bounded_context::{FormalBoundedContextV1, FormalBoundedWorkV1},
    },
    incremental_artifacts::OriginalArtifactRefV1,
    incremental_execution::RootSetV1,
};
use oasis7_world_state::{
    incremental_cognition_retention_ledger::CognitionRetentionWorkV1,
    incremental_field_member::{
        MemberPayloadCodecV1, MemberRecordLimitsV1, MemberRecordWriteContext, WorldFieldKeyV1,
        WorldFieldRecordKindV1, read_member_record_v1,
    },
    incremental_json_records::{JsonChangeWorkV1, JsonReadWorkV1, JsonWriteWorkV1},
    incremental_runtime_artifact_content::{
        RUNTIME_ARTIFACT_ORIGINAL_BYTES, RuntimeArtifactContentFieldV1,
        prepare_runtime_artifact_content_genesis_with_work_v1, runtime_artifact_original_from_v1,
    },
    incremental_runtime_revocation::BusinessDiffProcessingWorkV1,
    incremental_semantic_key::{hash_semantic_key_v1, write_semantic_key_v1},
    incremental_world_format::complete_world_format_profile_v1,
};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[path = "formal_iws_genesis_artifact_catalog_test_support.rs"]
mod cold_catalog;
use cold_catalog::{ExpectedArtifact, check_cold_wasm_catalog};

const STEPS: u64 = 256_000_000;
const BYTES: u64 = 4 << 30;
const PATHS: [&str; 3] = ["source-pages", "primary-pages", "replica-pages"];
const WASM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/iws_rule_transfer_candidate/m1.rule.transfer.wasm"
));
const WASM_SHA256: &str = "4db769768e3bde644cc2e34d65ae61c8ab2776b678541cfe5ddd15b467e6a504";
const SECOND_WASM: &[u8] = b"\0asm\x01\0\0\0";
const SECOND_WASM_SHA256: &str = "93a44bbb96c751218e4c00d479e4c14358122a389acca16205b1e4d0dc5f9476";

fn artifact_source(cfg: &GenesisConfigV1, include_second: bool) -> World {
    assert_eq!(WASM.len(), 407277);
    assert_eq!(&WASM[..8], b"\0asm\x01\0\0\0");
    let mut original = World::new_with_state(WorldState::default());
    // This real public API checks SHA-256 and installs these very module bytes.
    // No worker, module activation, proposal or signed operation is involved.
    original
        .register_module_artifact(WASM_SHA256, WASM)
        .unwrap();
    if include_second {
        // A valid empty Wasm module, through the same original SHA-256 gate.
        // Its distinct content ID makes the next Catalog append read its parent.
        original
            .register_module_artifact(SECOND_WASM_SHA256, SECOND_WASM)
            .unwrap();
        assert!(WASM_SHA256 < SECOND_WASM_SHA256);
    }
    original
        .bind_cognition_runtime(&cfg.world_id, "main", 0, None, "pending", 0)
        .unwrap();
    original.initialize_empty_formal_history().unwrap();
    let genesis = hex::encode(cfg.digest().unwrap());
    // Initial test input only, under the original input admission. It never
    // becomes formal material or a substitute for an accepted parent.
    let snapshot = original.snapshot_with_chain_resource_context(
        oasis7_runtime::ChainResourceDerivationContext {
            world_id: &cfg.world_id,
            chain_id: "projected-genesis-chain",
            genesis_ref: Some(&genesis),
            created_at_height: 0,
            manifest_height: 0,
            commit_block_hash: None,
            tick: original.state().time,
        },
        hex::encode(cfg.initial_content),
        hex::encode(cfg.format_profile),
    );
    let mut world = World::from_snapshot(snapshot, original.journal().clone()).unwrap();
    world.initialize_empty_formal_history().unwrap();
    assert!(world.state().agents.is_empty());
    assert_eq!(world.pending_actions_len(), 0);
    assert!(world.journal().events.is_empty());
    assert!(world.snapshot_catalog().records.is_empty());
    assert!(world.tick_consensus_records().is_empty());
    assert!(world.tick_consensus_rejection_audit_events().is_empty());
    assert_eq!(world.borrow_iws_runtime_history_v1().archived_tick_count, 0);
    assert!(
        world
            .capability_revocation_state()
            .world_service_results
            .is_empty()
    );
    world
}

fn json_read_work() -> JsonReadWorkV1 {
    JsonReadWorkV1 {
        cells_remaining: 1_000_000,
        json_nodes_remaining: 1_000_000,
        max_depth: 64,
        max_payload_bytes: 8 << 20,
    }
}

fn object_lanes(work: &IwsGenesisTransferWorkV1) -> [[u64; 7]; 3] {
    std::array::from_fn(|i| {
        [
            work.objects_physical[i].reads_remaining,
            work.objects_physical[i].writes_remaining,
            work.objects_logical[i].nodes_remaining,
            work.objects_physical[i].read_bytes_remaining,
            work.objects_physical[i].write_bytes_remaining,
            work.objects_logical[i].read_bytes_remaining,
            work.objects_logical[i].write_bytes_remaining,
        ]
    })
}

// Independently declared original finite recovery accounts, never a refill of
// any production account. Raw output uses half the original 16MiB logical sum.
struct Inspection {
    physical: [PageIoBudget; 3],
    logical: [ValueIoBudget; 3],
    stop: AtomicBool,
}
impl Inspection {
    fn new() -> Self {
        Self {
            physical: std::array::from_fn(|_| PageIoBudget {
                reads_remaining: 100_000,
                read_bytes_remaining: 16 << 20,
                writes_remaining: 0,
                write_bytes_remaining: 0,
            }),
            logical: std::array::from_fn(|_| ValueIoBudget {
                nodes_remaining: 100_000,
                read_bytes_remaining: 8 << 20,
                write_bytes_remaining: 8 << 20,
            }),
            stop: AtomicBool::new(false),
        }
    }
    fn check_fresh(&mut self, stores: &[IncrementalPageStore; 3]) {
        for (i, store) in stores.iter().enumerate() {
            let error = match IncrementalObjectStore::open(
                store,
                MemoryClass::Recovery,
                &mut self.physical[i],
                &self.stop,
            ) {
                Err(error) => error,
                Ok(_) => panic!("production must create the fresh object capability"),
            };
            assert!(matches!(error, PageStoreError::Io(ref e)
                if e.kind() == std::io::ErrorKind::NotFound));
        }
    }
    fn has_original(&mut self, store: &IncrementalPageStore, i: usize, id: [u8; 32]) -> bool {
        let objects = IncrementalObjectStore::open(
            store,
            MemoryClass::Recovery,
            &mut self.physical[i],
            &self.stop,
        )
        .unwrap();
        let mut io = ObjectIo {
            physical: &mut self.physical[i],
            logical: &mut self.logical[i],
            class: MemoryClass::Recovery,
            cancelled: &self.stop,
        };
        let present = match objects.read_object_owned(&id, WASM.len(), &mut io) {
            Ok(bytes) => {
                assert!(bytes.uses_budget(store.memory_budget()));
                assert_eq!(bytes.as_slice(), WASM);
                assert_eq!(*blake3::hash(&bytes).as_bytes(), id);
                true
            }
            Err(PageStoreError::Missing(missing)) => {
                assert_eq!(missing, id);
                false
            }
            Err(error) => panic!("unexpected actual original read: {error:?}"),
        };
        assert_eq!(self.physical[i].writes_remaining, 0);
        assert_eq!(self.physical[i].write_bytes_remaining, 0);
        present
    }
}

fn check_state_binding(
    store: &IncrementalPageStore,
    roots: &RootSetV1,
    expected: OriginalArtifactRefV1,
    original_key: &str,
    inspection: &mut Inspection,
    i: usize,
) {
    let state = store
        .state_session_with_cancelled(work::physical_io(), MemoryClass::Recovery, &inspection.stop)
        .unwrap();
    let values = store.value_session_with_cancelled(work::physical_io(), &inspection.stop);
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let semantic = hash_semantic_key_v1(
        original_key,
        store.memory_budget(),
        MemoryClass::Recovery,
        &mut keys,
        || false,
    )
    .unwrap();
    let key = WorldFieldKeyV1::new(
        RuntimeArtifactContentFieldV1::Original.descriptor(),
        WorldFieldRecordKindV1::Bytes,
        Some(semantic.digest),
    )
    .unwrap();
    let row = roots
        .state_root(StateNamespaceV1::Artifacts)
        .get(key.as_bytes(), &state)
        .unwrap()
        .unwrap();
    let mut context = ValueReadContext {
        source: &values,
        memory: store.memory_budget(),
        class: MemoryClass::Recovery,
        io: &mut inspection.logical[i],
    };
    let mut key_bytes = [0u8; 128];
    let mut key_output = std::io::Cursor::new(key_bytes.as_mut_slice());
    let mut binding = [0; RUNTIME_ARTIFACT_ORIGINAL_BYTES];
    let info = read_member_record_v1(
        &row,
        &key,
        &mut context,
        MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: RUNTIME_ARTIFACT_ORIGINAL_BYTES as u64,
            record_bytes: 8192,
        },
        &mut key_output,
        &mut std::io::Cursor::new(binding.as_mut_slice()),
        || false,
    )
    .unwrap();
    assert_eq!(info.codec, MemberPayloadCodecV1::Bytes);
    assert_eq!(info.payload_bytes, RUNTIME_ARTIFACT_ORIGINAL_BYTES as u64);
    let n = key_output.position() as usize;
    let mut expected_key = Vec::new();
    write_semantic_key_v1(original_key, &mut expected_key).unwrap();
    assert_eq!(&key_bytes[..n], expected_key);
    assert_eq!(
        runtime_artifact_original_from_v1(&binding).unwrap(),
        expected
    );
    assert!(state.take_read_error().is_none());
}

fn artifact_fanout(original_limit: Option<u64>, include_second: bool) {
    assert!(original_limit.is_none() || !include_second);
    let first_count_failure = original_limit == Some(0);
    let fixture = Fixture::new();
    let memory: [MemoryBudget; 3] =
        std::array::from_fn(|_| MemoryBudget::new(512 << 20, 32 << 20).unwrap());
    let input = memory[0].reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut cfg = config();
    cfg.format_profile = complete_world_format_profile_v1();
    let world = artifact_source(&cfg, include_second);
    let originals = [(WASM_SHA256, WASM), (SECOND_WASM_SHA256, SECOND_WASM)].map(|(key, bytes)| {
        ExpectedArtifact {
            key,
            bytes,
            reference: OriginalArtifactRefV1 {
                id: *blake3::hash(bytes).as_bytes(),
                byte_len: bytes.len() as u64,
            },
        }
    });
    let expected = &originals[..if include_second { 2 } else { 1 }];
    let cancelled = AtomicBool::new(false);
    let stores: [IncrementalPageStore; 3] = std::array::from_fn(|i| {
        IncrementalPageStore::create(
            &fixture.0.join(PATHS[i]),
            memory[i].clone(),
            PageCompression::Raw,
        )
        .unwrap()
    });
    let mut inspection = Inspection::new();
    inspection.check_fresh(&stores);
    let core = FormalBoundedWorkV1::new(STEPS, BYTES).unwrap();
    let identity = core.identity();
    let mut stop = || cancelled.load(Ordering::Acquire);
    let mut common = FormalBoundedContextV1::new(&memory[0], &core, &mut stop);
    let mut transfer = transfer_work();
    let mut producer = work::work();
    let producer_before = (
        producer.typed.artifact_originals.objects_remaining,
        producer.typed.artifact_originals.bytes_remaining,
    );
    if let Some(limit) = original_limit {
        transfer.originals_remaining = limit;
    }
    let before_count = transfer.originals_remaining;
    let before_lanes = object_lanes(&transfer);
    let mut logical = work::logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut json = json_read_work();
    let mut json_producer = JsonWriteWorkV1 {
        cells_remaining: 1_000_000,
        json_nodes_remaining: 1_000_000,
        max_json_depth: 64,
        max_pending_node_bytes: 8 << 20,
    };
    let mut changed = JsonChangeWorkV1 {
        json: json_read_work(),
        paths_remaining: 1_000_000,
        max_pending_pairs: 4096,
    };
    let mut processing = BusinessDiffProcessingWorkV1 {
        steps_remaining: 1_000_000,
        bytes_remaining: 64 << 20,
    };
    let baseline = memory.each_ref().map(MemoryBudget::used);
    let result = prepare_projected_iws_genesis_for_endpoints_v1(
        &world,
        &stores[0],
        [&stores[1], &stores[2]],
        &cfg,
        &mut RuntimeGenesisPhysicalContextV1 {
            memory: &memory[0],
            values: &mut logical,
            keys: &mut keys,
            limits: limits(),
            cancelled: &cancelled,
        },
        &mut producer,
        work::physical(),
        &mut transfer,
        &mut CognitionRetentionWorkV1 {
            json: &mut json,
            producer: &mut json_producer,
            changed: &mut changed,
            processing: &mut processing,
        },
        &mut common,
    );
    if let Err(ref error) = result {
        eprintln!("GEN_PREPARE_ERROR {error:?}");
    }
    assert_eq!(core.identity(), identity);
    assert!(!core.is_child_active());
    assert!(core.remaining_steps() < STEPS);
    assert!(core.remaining_read_bytes() < BYTES);
    assert_eq!(
        producer_before.0 - producer.typed.artifact_originals.objects_remaining,
        expected.len() as u64
    );
    assert_eq!(
        producer_before.1 - producer.typed.artifact_originals.bytes_remaining,
        expected
            .iter()
            .map(|artifact| artifact.bytes.len() as u64)
            .sum::<u64>()
    );
    let after_lanes = object_lanes(&transfer);
    for i in 0..3 {
        for j in 0..7 {
            assert!(after_lanes[i][j] <= before_lanes[i][j]);
        }
    }
    let material = if first_count_failure {
        // P fails before the later RootSet/business phase. Only earlier real
        // field pages may remain; this is not a completed business prefix.
        assert!(matches!(
            result,
            Err(IwsGenesisTransferErrorV1::Body(BodyMutationError::Invalid(
                "IWS original transfer occurrences"
            )))
        ));
        assert_eq!(transfer.originals_remaining, 0);
        for i in 0..3 {
            for j in [2, 5, 6] {
                assert_eq!(after_lanes[i][j], before_lanes[i][j]);
            }
        }
        assert_eq!(memory.each_ref().map(MemoryBudget::used), baseline);
        None
    } else if original_limit == Some(1) {
        // The real P original has finished in all three namespaces. The next
        // occurrence is the first Catalog page, so its zero account rejects
        // before a page or the final Catalog descriptor can be published.
        assert!(matches!(
            result,
            Err(IwsGenesisTransferErrorV1::Body(BodyMutationError::Invalid(
                "IWS original transfer occurrences"
            )))
        ));
        assert_eq!(before_count, 1);
        assert_eq!(transfer.originals_remaining, 0);
        for i in 0..3 {
            assert!(after_lanes[i][1] < before_lanes[i][1]);
        }
        assert!(core.remaining_steps() > 0);
        assert!(core.remaining_read_bytes() > 0);
        assert_eq!(memory.each_ref().map(MemoryBudget::used), baseline);
        None
    } else {
        assert!(original_limit.is_none());
        let material = result.unwrap();
        material.validate_context(&mut common).unwrap();
        assert_eq!(
            before_count - transfer.originals_remaining,
            expected.len() as u64 * (1 + 257)
                + 1
                + 5
                + material.delta().delta().node_writes.len() as u64
        );
        let projected = material.projected();
        let source = projected.business_source_bounded_v1(&mut common).unwrap();
        assert_eq!(source.world_result_count(), 0);
        drop(source);
        let (config_ref, config_wire) = material.config_original();
        assert_eq!(config_ref.id, projected.roots().genesis_config);
        assert_eq!(config_ref.byte_len, config_wire.len() as u64);
        assert_eq!(
            GenesisConfigV1::from_canonical_bytes(config_wire).unwrap(),
            cfg
        );
        assert_eq!(
            material.roots_original().0.id,
            projected.retention_manifest().roots
        );
        Some(material)
    };
    let settled = (core.remaining_steps(), core.remaining_read_bytes());
    drop(stores);
    let stores: [IncrementalPageStore; 3] = std::array::from_fn(|i| {
        IncrementalPageStore::open(
            &fixture.0.join(PATHS[i]),
            memory[i].clone(),
            PageCompression::Raw,
            MemoryClass::Recovery,
        )
        .unwrap()
    });
    for (i, store) in stores.iter().enumerate() {
        assert_eq!(
            inspection.has_original(store, i, expected[0].reference.id),
            !first_count_failure
        );
        if let Some(material) = &material {
            for artifact in expected {
                check_state_binding(
                    store,
                    material.projected().roots(),
                    artifact.reference,
                    artifact.key,
                    &mut inspection,
                    i,
                );
            }
            let objects = IncrementalObjectStore::open(
                store,
                MemoryClass::Recovery,
                &mut inspection.physical[i],
                &inspection.stop,
            )
            .unwrap();
            let mut io = ObjectIo {
                physical: &mut inspection.physical[i],
                logical: &mut inspection.logical[i],
                class: MemoryClass::Recovery,
                cancelled: &inspection.stop,
            };
            for (reference, wire) in [
                material.descriptor_original(),
                material.delta_original(),
                material.retention_original(),
                material.roots_original(),
                material.config_original(),
                material.wasm_catalog_original(),
            ] {
                assert!(wire.uses_budget(&memory[0]));
                assert_eq!(reference.byte_len, wire.len() as u64);
                let actual = objects
                    .read_object_owned(&reference.id, wire.len(), &mut io)
                    .unwrap();
                assert_eq!(actual.as_slice(), wire.as_slice());
                assert_eq!(*blake3::hash(&actual).as_bytes(), reference.id);
            }
        }
        check_cold_wasm_catalog(store, material.as_ref(), expected, &mut inspection, i);
    }
    assert_eq!(object_lanes(&transfer), after_lanes);
    assert_eq!(
        (core.remaining_steps(), core.remaining_read_bytes()),
        settled
    );
    drop(material);
    assert_eq!(memory.each_ref().map(MemoryBudget::used), baseline);
    drop(stores);
    drop(world);
    drop(input);
    assert_eq!(memory.each_ref().map(MemoryBudget::used), [0; 3]);
}

#[test]
fn complete_projected_genesis_actual_wasm_cold_reads_from_three_pools() {
    artifact_fanout(None, false);
}

#[test]
fn second_actual_wasm_reads_persisted_catalog_parent_and_cold_recovers_both_originals() {
    artifact_fanout(None, true);
}

#[test]
fn first_raw_artifact_count_failure_keeps_actual_hash_debit_without_owner_or_refund() {
    artifact_fanout(Some(0), false);
}

#[test]
fn first_catalog_page_count_failure_keeps_three_originals_without_owner_or_catalog_root() {
    artifact_fanout(Some(1), false);
}

#[test]
fn raw_artifact_child_preserves_real_prefix_first_error_cancellation_and_unwind() {
    // The direct public WorldState producer supplies the actual reference to
    // the immediate Raw consumer. No independently calculated ID enters Raw.
    for mode in 0..3 {
        let fixture = Fixture::new();
        let memory: [MemoryBudget; 3] =
            std::array::from_fn(|_| MemoryBudget::new(512 << 20, 32 << 20).unwrap());
        let input = memory[0].reserve(32 << 20, MemoryClass::Ordinary).unwrap();
        let registered = BTreeSet::from([WASM_SHA256.to_owned()]);
        let original_bytes = BTreeMap::from([(WASM_SHA256.to_owned(), Arc::from(WASM))]);
        let expected = *blake3::hash(WASM).as_bytes();
        let atomic = AtomicBool::new(false);
        let stores: [IncrementalPageStore; 3] = std::array::from_fn(|i| {
            IncrementalPageStore::create(
                &fixture.0.join(PATHS[i]),
                memory[i].clone(),
                PageCompression::Raw,
            )
            .unwrap()
        });
        let mut transfer = transfer_work();
        let objects: [IncrementalObjectStore<'_>; 3] = std::array::from_fn(|i| {
            IncrementalObjectStore::create(&stores[i], &mut transfer.objects_physical[i], &atomic)
                .unwrap()
        });
        if mode == 0 {
            transfer.objects_physical[1].writes_remaining = 0;
        }
        let before_lanes = object_lanes(&transfer);
        let before_count = transfer.originals_remaining;
        let before_canonical = (
            transfer.canonical.visits_remaining,
            transfer.canonical.bytes_remaining,
        );
        let core = FormalBoundedWorkV1::new(STEPS, BYTES).unwrap();
        let identity = core.identity();
        let raw_phase = Cell::new(false);
        let polls = Cell::new(0usize);
        let mut stop = || {
            // This unique child is Raw's six endpoint boundary polls only.
            // Prehash and the entry/frame admissions have no active child and
            // do not contribute a global or full-GEN cancellation threshold.
            if !raw_phase.get() || !core.is_child_active() {
                return false;
            }
            let poll = polls.get() + 1;
            polls.set(poll);
            if mode == 2 && poll == 2 {
                panic!("actual Raw source post-boundary unwind");
            }
            (mode == 1 && poll == 2) || (mode == 0 && poll >= 4)
        };
        let store_error = RefCell::new(None);
        let body_error = RefCell::new(None);
        let prehash_error = RefCell::new(None);
        let hook_calls = Cell::new(0u64);
        let hook_bytes = Cell::new(0u64);
        let p_calls = Cell::new(0usize);
        let mut original_work = work::work();
        let before_source = (
            original_work.typed.artifact_originals.objects_remaining,
            original_work.typed.artifact_originals.bytes_remaining,
        );
        let mut logical = work::logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 64 << 20,
        };
        struct Discard;
        impl oasis7_formal_protocol::incremental_value::ValueNodeWriter for Discard {
            fn put_node(
                &mut self,
                id: [u8; 32],
                bytes: ReservedValue<Vec<u8>>,
            ) -> Result<(), ValueTreeError> {
                assert_eq!(blake3::hash(&bytes).as_bytes(), &id);
                Ok(())
            }
        }
        let baseline = memory.each_ref().map(MemoryBudget::used);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut common = FormalBoundedContextV1::new(&memory[0], &core, &mut stop);
            let shared = RefCell::new(&mut common);
            let debit = |steps, bytes| {
                hook_calls.set(hook_calls.get() + 1);
                hook_bytes.set(hook_bytes.get() + bytes);
                shared.borrow_mut().charge(steps, bytes).map_err(|error| {
                    *prehash_error.borrow_mut() = Some(error);
                    ValueTreeError::Invalid("test actual prehash Common refusal")
                })
            };
            prepare_runtime_artifact_content_genesis_with_work_v1(
                &registered,
                &original_bytes,
                &mut MemberRecordWriteContext {
                    sink: &mut Discard,
                    memory: &memory[0],
                    io: &mut logical,
                    key_work: &mut keys,
                    limits: limits(),
                    cancelled: || atomic.load(Ordering::Acquire),
                },
                &mut original_work.typed.records_remaining,
                &mut original_work.typed.artifact_originals,
                (
                    &mut |reference: OriginalArtifactRefV1, bytes: &[u8]| {
                        p_calls.set(p_calls.get() + 1);
                        assert_eq!(reference.id, expected);
                        assert_eq!(reference.byte_len, WASM.len() as u64);
                        assert_eq!(bytes, WASM);
                        raw_phase.set(true);
                        originals::Originals {
                            stores: &objects,
                            physical: &mut transfer.objects_physical,
                            logical: &mut transfer.objects_logical,
                            occurrences: &mut transfer.originals_remaining,
                            max_bytes: transfer.max_original_bytes,
                            cancelled: &atomic,
                            error: &store_error,
                        }
                        .raw_bounded(reference, bytes, &mut shared.borrow_mut())
                        .map(|actual| {
                            assert_eq!(actual, reference);
                        })
                        .map_err(|error| {
                            *body_error.borrow_mut() = Some(error);
                            ValueTreeError::Invalid("test actual Raw transfer refused")
                        })
                    },
                    Some(&debit),
                ),
                &mut |_| Ok(()),
            )
        }));
        raw_phase.set(false);
        assert!(prehash_error.borrow().is_none());
        assert_eq!(hook_calls.get(), 9);
        assert_eq!(hook_bytes.get(), 415509);
        assert_eq!(p_calls.get(), 1);
        assert_eq!(
            before_source.0 - original_work.typed.artifact_originals.objects_remaining,
            1
        );
        assert_eq!(
            before_source.1 - original_work.typed.artifact_originals.bytes_remaining,
            WASM.len() as u64
        );
        assert_eq!(core.identity(), identity);
        assert!(!core.is_child_active());
        assert_eq!(transfer.originals_remaining, before_count - 1);
        assert_eq!(
            (
                transfer.canonical.visits_remaining,
                transfer.canonical.bytes_remaining
            ),
            before_canonical
        );
        assert_eq!(memory.each_ref().map(MemoryBudget::used), baseline);
        if mode == 2 {
            assert!(result.is_err());
            assert!(body_error.borrow().is_none());
            assert!(store_error.borrow().is_none());
            assert_eq!(polls.get(), 2);
            assert_eq!(core.remaining_steps(), 0);
            assert_eq!(core.remaining_read_bytes(), 0);
            assert_eq!(object_lanes(&transfer), [[0; 7]; 3]);
        } else {
            assert!(result.unwrap().is_err());
            let error = body_error.borrow_mut().take().unwrap();
            if mode == 0 {
                assert!(matches!(
                    error,
                    BodyMutationError::Invalid("IWS original replica storage failed")
                ));
                assert!(matches!(
                    &*store_error.borrow(),
                    Some(PageStoreError::ResourceLimited("physical write allowance"))
                ));
                // A post-primary cancellation would be poll 4. Actual writer
                // failure returns first, and cannot be overwritten by it.
                assert_eq!(polls.get(), 3);
            } else {
                let BodyMutationError::Original(error) = error else {
                    panic!("lost Common cancellation: {error:?}");
                };
                assert!(
                    matches!(error.downcast_ref::<FormalError>(), Some(FormalError::Invalid(note))
                    if note == "bounded archive read cancelled")
                );
                assert!(store_error.borrow().is_none());
                assert_eq!(polls.get(), 2);
                assert_eq!(&object_lanes(&transfer)[1..], &before_lanes[1..]);
            }
            assert!(core.remaining_steps() < STEPS);
            assert!(core.remaining_read_bytes() < BYTES);
            assert!(core.remaining_steps() > 0);
            assert!(core.remaining_read_bytes() > 0);
            let after = object_lanes(&transfer);
            for i in 0..3 {
                for j in 0..7 {
                    assert!(after[i][j] <= before_lanes[i][j]);
                }
            }
            assert!(after[0][1] < before_lanes[0][1]);
        }
        let after = object_lanes(&transfer);
        let settled = (core.remaining_steps(), core.remaining_read_bytes());
        drop(objects);
        drop(stores);
        let stores: [IncrementalPageStore; 3] = std::array::from_fn(|i| {
            IncrementalPageStore::open(
                &fixture.0.join(PATHS[i]),
                memory[i].clone(),
                PageCompression::Raw,
                MemoryClass::Recovery,
            )
            .unwrap()
        });
        let mut inspection = Inspection::new();
        for (i, store) in stores.iter().enumerate() {
            // Source really completed its full publish/readback before the
            // cancellation/unwind; primary and replica have no original ID.
            assert_eq!(inspection.has_original(store, i, expected), i == 0);
        }
        assert_eq!(object_lanes(&transfer), after);
        assert_eq!(
            (core.remaining_steps(), core.remaining_read_bytes()),
            settled
        );
        drop(stores);
        drop(original_bytes);
        drop(registered);
        drop(input);
        assert_eq!(memory.each_ref().map(MemoryBudget::used), [0; 3]);
    }
}
