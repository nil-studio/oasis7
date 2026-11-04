#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget,
    objects::{IncrementalObjectStore, ObjectIo},
};
use oasis7_formal_protocol::{
    controlled_authority::{
        formal::{
            self,
            test_fixtures::{self as fixture, incremental},
        },
        replicated_protocol::ArtifactRole,
    },
    incremental_execution::{ExecutionFactKindV1, ExecutionFactRefV1},
    incremental_history::{CatalogRootV1, HeaderPreparation},
    incremental_material::*,
    incremental_state::*,
    incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_runtime::{
    incremental_runtime_history_original::*, incremental_runtime_history_source::*,
};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use oasis7_world_state::{
    incremental_field_member::*, incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_history_index::*, incremental_runtime_history_manifest::*,
    snapshot::SnapshotRecord,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "oasis7-runtime-history-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
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
fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 4_000_000,
        read_bytes_remaining: 512 << 20,
        writes_remaining: 1_000_000,
        write_bytes_remaining: 512 << 20,
    }
}
fn logical() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 512 << 20,
        write_bytes_remaining: 512 << 20,
    }
}
fn core_budget() -> CoreBudget {
    CoreBudget {
        objects_remaining: 128,
        original_bytes_remaining: 16 << 20,
        new_pages_remaining: 4096,
        merkle_reads_remaining: 100000,
    }
}
fn facts() -> FactBudget {
    FactBudget {
        facts_remaining: 128,
        original_bytes_remaining: 16 << 20,
        max_fact_bytes: 4 << 20,
    }
}
fn work() -> RuntimeHistoryIndexWorkV1 {
    RuntimeHistoryIndexWorkV1 {
        cells_remaining: 100000,
        max_pending_node_bytes: 1 << 20,
    }
}
fn canonical_work() -> CanonicalCborWorkV1 {
    CanonicalCborWorkV1 {
        visits_remaining: 100000,
        bytes_remaining: 16 << 20,
        max_depth: 64,
        max_map_keys: 1024,
        max_key_bytes: 1024,
    }
}
fn put(objects: &IncrementalObjectStore<'_>, bytes: &[u8], io: &mut ObjectIo<'_>) {
    objects
        .write_object(
            &mut &bytes[..],
            bytes.len() as u64,
            Some(*blake3::hash(bytes).as_bytes()),
            io,
        )
        .unwrap();
}
fn state_error(error: oasis7_distfs::incremental_page_store::PageStoreError) -> ValueTreeError {
    ValueTreeError::Io(std::io::Error::other(error))
}
#[test]
fn runtime_history_actual_signed_snapshot_fact_reopens_in_fresh_pool() {
    for (compression, attack) in [
        (PageCompression::Raw, false),
        (PageCompression::Zstd, false),
        (PageCompression::Raw, true),
    ] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let (head, roots, manifest_bytes, location, config, authority) = {
            let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
            let _fixture = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            let s = incremental::setup_with_memory(memory.clone());
            let cancelled = AtomicBool::new(false);
            let mut physical_io = physical();
            let mut logical_io = logical();
            let objects =
                IncrementalObjectStore::create(&store, &mut physical_io, &cancelled).unwrap();
            let mut io = ObjectIo {
                physical: &mut physical_io,
                logical: &mut logical_io,
                class: MemoryClass::Ordinary,
                cancelled: &cancelled,
            };
            let snapshot = SnapshotRecord {
                snapshot_hash: "actual snapshot hash".into(),
                journal_len: usize::MAX,
                created_at: u64::MAX,
                manifest_hash: "actual manifest hash".into(),
            };
            let binding = measure_runtime_history_original_v1(
                RuntimeHistoryOriginalRefV1::Snapshot(&snapshot),
                &memory,
                &mut canonical_work(),
                &mut || false,
                1 << 20,
            )
            .unwrap();
            let mut raw = Vec::new();
            with_runtime_history_original_v1(
                RuntimeHistoryOriginalRefV1::Snapshot(&snapshot),
                &memory,
                &mut canonical_work(),
                &mut || false,
                1 << 20,
                |producer| {
                    producer.persist_to(
                        &mut |stream: &mut dyn oasis7_runtime::AgentEventOriginalStreamV1,
                              _: &mut dyn FnMut() -> bool| {
                            stream.write_to(&mut raw)
                        },
                    )
                },
            )
            .unwrap();
            put(&objects, &raw, &mut io);
            let ordinal = s.history.facts.facts.len() as u64;
            let row = RuntimeHistoryIndexRowV1 {
                location: FactLocationV1 {
                    height: 0,
                    ordinal,
                    original: binding.original_id(),
                },
                encoded_bytes: binding.original_bytes(),
            };
            let mut writer = store.value_session(physical());
            let mut value_io = logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut write = MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut value_io,
                key_work: &mut keys,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 4096,
                    payload_bytes: 8192,
                    record_bytes: 8192,
                },
                cancelled: || false,
            };
            let lanes = RuntimeHistoryLaneV1::ALL.map(|lane| {
                let count = u64::from(lane == RuntimeHistoryLaneV1::Snapshot);
                let root = prepare_runtime_history_index_v1(
                    lane,
                    count,
                    (0..count).map(|_| Ok(row)),
                    &mut write,
                    &mut work(),
                )
                .unwrap();
                RuntimeHistoryWindowV1 {
                    root,
                    total: count,
                    start: 0,
                    count: if attack { 0 } else { count },
                }
            });
            let manifest = RuntimeHistoryManifestV1 {
                roots: s.roots.digest().unwrap(),
                previous_view: None,
                lanes,
                max_snapshots: 10,
                archived_tick_count: 0,
            };
            let bytes = manifest.canonical_bytes().unwrap();
            put(&objects, &bytes, &mut io);
            let mut genesis = s.history.facts.facts.clone();
            genesis.push(ExecutionFactRefV1 {
                kind: ExecutionFactKindV1::Result,
                id: binding.original_id(),
            });
            let selected_ordinal = genesis.len() as u64;
            genesis.push(ExecutionFactRefV1 {
                kind: ExecutionFactKindV1::Result,
                id: *blake3::hash(&bytes).as_bytes(),
            });
            let history = HeaderPreparation {
                config: &s.config,
                catalog: &CatalogRootV1::empty(s.config.scope().unwrap()).unwrap(),
                roots: &s.roots,
                parent: None,
                parent_history: None,
                transition: None,
                genesis_facts: &genesis,
                operation: s.history.body.operation,
                authority: s.history.body.authority,
                execution_environment: s.config.execution_environment,
                checkpoint: true,
            }
            .prepare(&memory)
            .unwrap();
            let (blobs, catalog) = incremental::core_objects(&s, &history);
            for blob in blobs.iter() {
                put(&objects, blob.as_slice(), &mut io);
            }
            {
                let mut no_cancel = || false;
                let mut pages = store
                    .buffered_page_writer(io.physical, &cancelled, &mut no_cancel)
                    .unwrap();
                for node in &catalog.new_nodes {
                    pages
                        .put_node(
                            *blake3::hash(&node.bytes).as_bytes(),
                            memory
                                .reserve(node.bytes.capacity() + 128, MemoryClass::Ordinary)
                                .unwrap()
                                .retain(node.bytes.clone()),
                        )
                        .unwrap();
                }
                pages.finish().unwrap();
            }
            let mut record = s.commit.record().clone();
            record.artifact_roots.insert(
                ArtifactRole::IncrementalCommitBody,
                hex::encode(history.body.digest().unwrap()),
            );
            record.artifact_roots.insert(
                ArtifactRole::IncrementalHistoryPrefix,
                hex::encode(history.prefix.digest().unwrap()),
            );
            let proof = fixture::proof(
                record,
                &s.authority,
                formal::FormalCompletionBranchV1::Normal,
                None,
                None,
                4,
                5,
                6,
            );
            let candidate = formal::verify_formal_candidate(
                &formal::SignedCanonicalCandidateV1 {
                    record: proof.decision.record.clone(),
                    writer_signature_hex: proof.decision.writer_signature_hex.clone(),
                },
                None,
                &s.authority,
                None,
                None,
                formal::FormalCompletionBranchV1::Normal,
            )
            .unwrap();
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let core = verify_commit_core(
                &CoreRequest {
                    candidate: &candidate,
                    original_authority: &s.authority,
                    parent: None,
                    max_pending_node_bytes: 4 << 20,
                },
                &mut objects.original_reader(&mut io),
                &state,
                &memory,
                &mut core_budget(),
                || false,
            )
            .unwrap();
            let originals = verify_fact_originals(
                &core,
                &mut objects.original_reader(&mut io),
                &memory,
                &mut facts(),
                || false,
            )
            .unwrap();
            let values = store.value_session(physical());
            let mut quota = logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut read = WorldFieldReadContextV1 {
                root: &manifest.lanes[0].root,
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut quota,
                },
                key_work: &mut keys,
                semantic_bytes: 4096,
                record_bytes: 8192,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            };
            let indexes = RuntimeHistoryLaneV1::ALL.map(|lane| {
                qualify_runtime_history_index_v1(
                    lane,
                    &manifest.lanes[lane.index()],
                    &mut read,
                    &mut work(),
                )
                .unwrap()
            });
            let rows = [RuntimeHistoryCandidateRowV1 {
                binding: &binding,
                fact_ordinal: ordinal,
                index_ordinal: 0,
            }];
            let checked = check_candidate_runtime_history_source_v1(
                &candidate,
                &core,
                &originals,
                &manifest,
                selected_ordinal,
                &indexes,
                None,
                &rows,
                &mut read,
                &mut RuntimeHistorySourceWorkV1 {
                    rows_remaining: 10,
                    index: work(),
                },
            )
            .unwrap();
            let wrong_snapshot = SnapshotRecord {
                manifest_hash: "different actual type value".into(),
                ..snapshot.clone()
            };
            let wrong = measure_runtime_history_original_v1(
                RuntimeHistoryOriginalRefV1::Snapshot(&wrong_snapshot),
                &memory,
                &mut canonical_work(),
                &mut || false,
                1 << 20,
            )
            .unwrap();
            assert!(
                check_candidate_runtime_history_source_v1(
                    &candidate,
                    &core,
                    &originals,
                    &manifest,
                    selected_ordinal,
                    &indexes,
                    None,
                    &[RuntimeHistoryCandidateRowV1 {
                        binding: &wrong,
                        fact_ordinal: ordinal,
                        index_ordinal: 0
                    }],
                    &mut read,
                    &mut RuntimeHistorySourceWorkV1 {
                        rows_remaining: 10,
                        index: work()
                    }
                )
                .is_err()
            );
            let accepted =
                formal::verify_formal_qualified(&proof, &s.authority, None, None).unwrap();
            let accepted_source =
                promote_runtime_history_source_v1(&checked, &accepted, &mut read, &mut work())
                    .unwrap();
            let location = accepted_source.occurrence().location();
            (
                accepted,
                s.roots.clone(),
                manifest.canonical_bytes().unwrap(),
                location,
                s.config,
                s.authority,
            )
        };
        // The producer DTOs, all qualifications and the entire old pool are gone.
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let _manifest_input = memory.reserve(8192, MemoryClass::Recovery).unwrap();
        let manifest = RuntimeHistoryManifestV1::from_canonical_bytes(&manifest_bytes).unwrap();
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Recovery)
                .unwrap();
        let cancelled = AtomicBool::new(false);
        let mut physical_io = physical();
        let mut logical_io = logical();
        let objects = IncrementalObjectStore::open(
            &store,
            MemoryClass::Recovery,
            &mut physical_io,
            &cancelled,
        )
        .unwrap();
        let state = store
            .state_session(physical(), MemoryClass::Recovery)
            .unwrap();
        let values = store.value_session(physical());
        let mut io = ObjectIo {
            physical: &mut physical_io,
            logical: &mut logical_io,
            class: MemoryClass::Recovery,
            cancelled: &cancelled,
        };
        let selected = verify_fact_occurrence_v1(
            &FactRequestV1 {
                kind: ExecutionFactKindV1::Result,
                head: &head,
                roots: &roots,
                location,
            },
            &mut FactReadContextV1 {
                originals: &mut objects.recovery_original_reader(&mut io).unwrap(),
                catalog_pages: &state,
                memory: &memory,
                class: MemoryClass::Recovery,
                core_budget: &mut core_budget(),
                fact_budget: &mut facts(),
                cancelled: || false,
            },
        )
        .unwrap();
        let mut quota = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut read = WorldFieldReadContextV1 {
            root: &manifest.lanes[0].root,
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut quota,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let recovered = recover_runtime_history_source_v1(
            &head,
            &roots,
            &manifest,
            selected,
            &mut read,
            &mut work(),
        )
        .unwrap();
        assert_eq!(recovered.allocation_class(), MemoryClass::Recovery);
        let ordinary =
            readmit_runtime_history_source_ordinary_v1(&recovered, &head, &roots, &memory, || {
                false
            })
            .unwrap();
        assert_eq!(ordinary.allocation_class(), MemoryClass::Ordinary);
        assert_eq!(ordinary.manifest(), &manifest);
        drop(read);
        drop(recovered);
        drop(io);
        drop(objects);
        drop(state);
        drop(values);
        drop(store);
        let ordinary_store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        append_snapshot_ordinary(
            &ordinary,
            &config,
            &authority,
            &head,
            &roots,
            &memory,
            &ordinary_store,
            attack,
        );
    }
}

fn append_snapshot_ordinary(
    parent_source: &oasis7_proto::memory_budget::ReservedValue<AcceptedRuntimeHistorySourceV1>,
    config: &oasis7_formal_protocol::incremental_history::GenesisConfigV1,
    authority: &formal::VerifiedAuthorityV1,
    parent: &formal::VerifiedFormalCommitV1,
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    memory: &MemoryBudget,
    store: &IncrementalPageStore,
    attack: bool,
) {
    use oasis7_formal_protocol::{
        incremental_execution::{ExecutionParent, ExecutionTransaction, TransactionLimits},
        incremental_history::{CommitBodyV1, HistoryPrefixV1},
    };
    let cancelled = AtomicBool::new(false);
    let mut physical_io = physical();
    let mut logical_io = logical();
    let objects =
        IncrementalObjectStore::open(store, MemoryClass::Ordinary, &mut physical_io, &cancelled)
            .unwrap();
    let mut io = ObjectIo {
        physical: &mut physical_io,
        logical: &mut logical_io,
        class: MemoryClass::Ordinary,
        cancelled: &cancelled,
    };
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut writer = store.value_session(physical());
    let body = CommitBodyV1::from_canonical_bytes(
        &objects
            .read_object_owned(&parent_source.occurrence().head_body_id(), 4096, &mut io)
            .unwrap(),
    )
    .unwrap();
    let prefix = HistoryPrefixV1::from_canonical_bytes(
        &objects
            .read_object_owned(&body.history, 4096, &mut io)
            .unwrap(),
    )
    .unwrap();
    let catalog = body
        .catalog_before
        .extend(&body, &state, memory, 4 << 20)
        .unwrap();
    let execution_parent = ExecutionParent {
        roots: roots.clone(),
        commit: body.digest().unwrap(),
        authority: body.authority,
    };
    let fence = execution_parent.fence().unwrap();
    let operation = [63; 32];
    let mut tx = ExecutionTransaction::begin(
        execution_parent,
        operation,
        &state,
        memory,
        TransactionLimits {
            max_reads: 4,
            max_writes: 2,
            max_facts: 4,
            max_pending_node_bytes: 4 << 20,
        },
    )
    .unwrap();
    let input = *blake3::hash(b"Runtime History actual ordinary input").as_bytes();
    put(&objects, b"Runtime History actual ordinary input", &mut io);
    tx.append_fact(ExecutionFactRefV1 {
        kind: ExecutionFactKindV1::Input,
        id: input,
    })
    .unwrap();
    let snapshot = SnapshotRecord {
        snapshot_hash: "new ordinary original snapshot".into(),
        journal_len: 1,
        created_at: roots.simulation_tick + 1,
        manifest_hash: "new ordinary manifest hash".into(),
    };
    let binding = measure_runtime_history_original_v1(
        RuntimeHistoryOriginalRefV1::Snapshot(&snapshot),
        memory,
        &mut canonical_work(),
        &mut || false,
        1 << 20,
    )
    .unwrap();
    let mut raw = Vec::new();
    with_runtime_history_original_v1(
        RuntimeHistoryOriginalRefV1::Snapshot(&snapshot),
        memory,
        &mut canonical_work(),
        &mut || false,
        1 << 20,
        |producer| {
            producer.persist_to(
                &mut |stream: &mut dyn oasis7_runtime::AgentEventOriginalStreamV1,
                      _: &mut dyn FnMut() -> bool| stream.write_to(&mut raw),
            )
        },
    )
    .unwrap();
    put(&objects, &raw, &mut io);
    if !attack {
        tx.append_fact(ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Result,
            id: binding.original_id(),
        })
        .unwrap();
    }
    let mut read_io = logical();
    let mut read_keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut read = WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Artifacts),
        state: &state,
        values: ValueReadContext {
            source: &values,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut read_io,
        },
        key_work: &mut read_keys,
        semantic_bytes: 4096,
        record_bytes: 8192,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    let mut write_io = logical();
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut writer,
        memory,
        io: &mut write_io,
        key_work: &mut write_keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 8192,
            record_bytes: 8192,
        },
        cancelled: || false,
    };
    let old = parent_source.index(RuntimeHistoryLaneV1::Snapshot);
    let index_ordinal = old.window().total;
    let next = if attack {
        let mut forged = old.window().clone();
        forged.count = forged.total - forged.start;
        qualify_runtime_history_index_v1(
            RuntimeHistoryLaneV1::Snapshot,
            &forged,
            &mut read,
            &mut work(),
        )
        .unwrap()
    } else {
        prepare_runtime_history_index_append_v1(
            old,
            RuntimeHistoryIndexRowV1 {
                location: FactLocationV1 {
                    height: roots.height + 1,
                    ordinal: 1,
                    original: binding.original_id(),
                },
                encoded_bytes: binding.original_bytes(),
            },
            &mut read,
            &mut write,
            &mut work(),
        )
        .unwrap()
    };
    let mut manifest = parent_source.manifest().clone();
    manifest.previous_view = Some(parent_source.occurrence().location().original);
    manifest.lanes[0] = next.window().clone();
    let guard = tx.guard();
    let sealed = tx
        .seal_with_final_fact(
            &guard,
            &fence,
            roots.simulation_tick + 1,
            || false,
            |after| {
                manifest.roots = after.digest().unwrap();
                let bytes = manifest.canonical_bytes().unwrap();
                put(&objects, &bytes, &mut io);
                Ok::<_, ValueTreeError>(ExecutionFactRefV1 {
                    kind: ExecutionFactKindV1::Result,
                    id: *blake3::hash(&bytes).as_bytes(),
                })
            },
        )
        .unwrap();
    let history = HeaderPreparation {
        config,
        catalog: &catalog.after,
        roots: &sealed.record.after,
        parent: Some(&body),
        parent_history: Some(&prefix),
        transition: Some(&sealed.record),
        genesis_facts: &[],
        operation,
        authority: body.authority,
        execution_environment: config.execution_environment,
        checkpoint: false,
    }
    .prepare(memory)
    .unwrap();
    let after_catalog = history
        .body
        .catalog_before
        .extend(&history.body, &state, memory, 4 << 20)
        .unwrap();
    {
        let mut no_cancel = || false;
        let mut pages = store
            .buffered_page_writer(io.physical, &cancelled, &mut no_cancel)
            .unwrap();
        for node in &after_catalog.new_nodes {
            pages
                .put_node(
                    *blake3::hash(&node.bytes).as_bytes(),
                    memory
                        .reserve(node.bytes.capacity() + 128, MemoryClass::Ordinary)
                        .unwrap()
                        .retain(node.bytes.clone()),
                )
                .unwrap();
        }
        pages.finish().unwrap();
    }
    for bytes in [
        history.body.canonical_bytes().unwrap(),
        history.prefix.canonical_bytes().unwrap(),
        history.segment.canonical_bytes().unwrap(),
        history.facts.canonical_bytes(memory).unwrap().to_vec(),
        sealed.record.after.canonical_bytes().unwrap(),
        history.transition_bytes.as_ref().unwrap().to_vec(),
        after_catalog.after.canonical_bytes().unwrap(),
    ] {
        put(&objects, &bytes, &mut io);
    }
    let mut frozen = fixture::frozen(authority, parent, None);
    frozen.reservation.body.schema_version = formal::incremental::IWS_FROZEN_SCHEMA;
    frozen.reservation.body.runtime_manifest_digest =
        parent.record().runtime_manifest_digest.clone();
    frozen.reservation.body.payload_digest = hex::encode(input);
    frozen.reservation.body.request_digest = hex::encode(operation);
    frozen.reservation.signature_hex = fixture::sign(
        formal::frozen_signing_bytes(&frozen.reservation.body).unwrap(),
        4,
    );
    let reservation = frozen.reservation.body.reservation_id().unwrap();
    for (receipt, key) in [(&mut frozen.primary, 5), (&mut frozen.replica, 6)] {
        receipt.body.reservation_id = reservation.clone();
        receipt.signature_hex = fixture::sign(
            formal::frozen_receipt_signing_bytes(&receipt.body).unwrap(),
            key,
        );
    }
    let mut record = fixture::execution(authority, &frozen);
    record.schema_version = formal::incremental::IWS_RECORD_SCHEMA;
    record.after_state_root = hex::encode(sealed.record.after.digest().unwrap());
    record.artifact_roots = parent.record().artifact_roots.clone();
    for (role, id) in [
        (ArtifactRole::Input, input),
        (
            ArtifactRole::IncrementalRootSet,
            sealed.record.after.digest().unwrap(),
        ),
        (
            ArtifactRole::IncrementalCommitBody,
            history.body.digest().unwrap(),
        ),
        (
            ArtifactRole::IncrementalHistoryPrefix,
            history.prefix.digest().unwrap(),
        ),
    ] {
        record.artifact_roots.insert(role, hex::encode(id));
    }
    let proof = fixture::proof(
        record,
        authority,
        formal::FormalCompletionBranchV1::Normal,
        None,
        Some(frozen),
        4,
        5,
        6,
    );
    let candidate = formal::verify_formal_candidate(
        &formal::SignedCanonicalCandidateV1 {
            record: proof.decision.record.clone(),
            writer_signature_hex: proof.decision.writer_signature_hex.clone(),
        },
        proof.decision.frozen.as_ref(),
        authority,
        Some(parent),
        None,
        formal::FormalCompletionBranchV1::Normal,
    )
    .unwrap();
    let core = verify_commit_core(
        &CoreRequest {
            candidate: &candidate,
            original_authority: authority,
            parent: Some(parent),
            max_pending_node_bytes: 4 << 20,
        },
        &mut objects.original_reader(&mut io),
        &state,
        memory,
        &mut core_budget(),
        || false,
    )
    .unwrap();
    let originals = verify_fact_originals(
        &core,
        &mut objects.original_reader(&mut io),
        memory,
        &mut facts(),
        || false,
    )
    .unwrap();
    let indexes = [
        next,
        qualify_runtime_history_index_v1(
            RuntimeHistoryLaneV1::Journal,
            &manifest.lanes[1],
            &mut read,
            &mut work(),
        )
        .unwrap(),
        qualify_runtime_history_index_v1(
            RuntimeHistoryLaneV1::Tick,
            &manifest.lanes[2],
            &mut read,
            &mut work(),
        )
        .unwrap(),
        qualify_runtime_history_index_v1(
            RuntimeHistoryLaneV1::Audit,
            &manifest.lanes[3],
            &mut read,
            &mut work(),
        )
        .unwrap(),
    ];
    let mut source_work = RuntimeHistorySourceWorkV1 {
        rows_remaining: 1,
        index: RuntimeHistoryIndexWorkV1 {
            cells_remaining: 4,
            max_pending_node_bytes: 1 << 20,
        },
    };
    let candidate_rows = [RuntimeHistoryCandidateRowV1 {
        binding: &binding,
        fact_ordinal: 1,
        index_ordinal,
    }];
    let checked = check_candidate_runtime_history_source_v1(
        &candidate,
        &core,
        &originals,
        &manifest,
        if attack { 1 } else { 2 },
        &indexes,
        Some(parent_source),
        if attack { &[] } else { &candidate_rows },
        &mut read,
        &mut source_work,
    );
    if attack {
        assert!(
            checked.is_err(),
            "actual ordinary candidate must not restore a truncated suffix with public fullQ and no new originals"
        );
        return;
    }
    let checked = checked.unwrap();
    assert_eq!(source_work.rows_remaining, 0);
    assert!(
        source_work.index.cells_remaining >= 3,
        "only new point is opened; no old prefix scan"
    );
    let accepted = formal::verify_formal_qualified(&proof, authority, Some(parent), None).unwrap();
    let source =
        promote_runtime_history_source_v1(&checked, &accepted, &mut read, &mut work()).unwrap();
    assert_eq!(
        source.manifest().lanes[0].total,
        parent_source.manifest().lanes[0].total + 1
    );
    assert_eq!(accepted.record().position, parent.record().position + 1);
}
