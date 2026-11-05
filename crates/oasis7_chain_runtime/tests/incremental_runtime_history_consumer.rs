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
fn production_history_reader_four_lanes_prepare_accept_cold_and_ordinary_delta() {
    for (compression, attack) in [
        (PageCompression::Raw, false),
        (PageCompression::Zstd, false),
    ] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let (head, roots, manifest_bytes, location, config, authority) = {
            let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
            let _fixture = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            let initial = incremental::setup_with_memory(memory.clone());
            let mut config = initial.config.clone();
            config.format_profile =
                oasis7_world_state::incremental_world_format::complete_world_format_profile_v1();
            let s = incremental::setup_with_config_and_state(
                memory.clone(),
                config,
                initial.roots.state.clone(),
                incremental::FixtureExecutionArtifactRefsV1 {
                    manifest: [5; 32],
                    wasm: [6; 32],
                    rules: [7; 32],
                    policy: [8; 32],
                },
            );
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
            let points = originals_for_four_lanes(0, 0, &memory, &objects, &mut io);
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
                let root = prepare_runtime_history_index_v1(
                    lane,
                    1,
                    std::iter::once(Ok(points[lane.index()])),
                    &mut write,
                    &mut work(),
                )
                .unwrap();
                RuntimeHistoryWindowV1 {
                    root,
                    total: 1,
                    start: 0,
                    count: 1,
                }
            });
            drop(write);
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
            for lane in RuntimeHistoryLaneV1::ALL {
                genesis.push(ExecutionFactRefV1 {
                    kind: if lane == RuntimeHistoryLaneV1::Journal {
                        ExecutionFactKindV1::Event
                    } else {
                        ExecutionFactKindV1::Result
                    },
                    id: points[lane.index()].location.original,
                });
            }
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
                root: s.roots.state_root(StateNamespaceV1::Allocator),
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
            let mut replay_io = logical();
            let mut replay_keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut replay = MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut replay_io,
                key_work: &mut replay_keys,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 4096,
                    payload_bytes: 8192,
                    record_bytes: 8192,
                },
                cancelled: || false,
            };
            let mut consume_work = consumer_work();
            let consumed = read_candidate_runtime_history_source_v1(
                &RuntimeHistoryCandidateReadRequestV1 {
                    candidate: &candidate,
                    core: &core,
                    originals: &originals,
                    manifest: &manifest,
                    selected_ordinal,
                    parent: None,
                    prefix: None,
                },
                &mut objects.original_reader(&mut io),
                &mut read,
                &mut replay,
                &mut consume_work,
            )
            .unwrap();
            assert_eq!(
                consume_work.originals.facts_remaining, 0,
                "exactly four real GEN original reads"
            );
            let prepared = prepare_runtime_history_source_promotion_v1(
                &consumed.source,
                consumed.indexes,
                &memory,
                || false,
            )
            .unwrap();
            let accepted =
                formal::verify_formal_qualified(&proof, &s.authority, None, None).unwrap();
            let blocker = memory
                .reserve((112 << 20) - memory.used(), MemoryClass::Ordinary)
                .unwrap();
            assert!(memory.reserve(1, MemoryClass::Ordinary).is_err());
            let used = memory.used();
            let accepted_source =
                finish_prepared_runtime_history_source_promotion_v1(prepared, &accepted).unwrap();
            assert_eq!(memory.used(), used, "fixed finish makes no new admission");
            drop(blocker);
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
        let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
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
            max_facts: 8,
            max_pending_node_bytes: 4 << 20,
        },
    )
    .unwrap();
    // The counter is authenticated by the actual current accepted Runtime
    // Result, before Input is appended. Input is not a WorldEvent journal row.
    let world_events =
        oasis7_runtime::WorldEventCounterV1::begin(parent_source, parent, &tx, memory).unwrap();
    let base_hot = parent_source.manifest().lanes[RuntimeHistoryLaneV1::Journal.index()].count;
    assert_eq!(world_events.journal_height(&tx).unwrap(), base_hot);
    let input = *blake3::hash(b"Runtime History actual ordinary input").as_bytes();
    put(&objects, b"Runtime History actual ordinary input", &mut io);
    tx.append_fact(ExecutionFactRefV1 {
        kind: ExecutionFactKindV1::Input,
        id: input,
    })
    .unwrap();
    assert_eq!(world_events.journal_height(&tx).unwrap(), base_hot);
    assert!(
        oasis7_runtime::WorldEventCounterV1::begin(parent_source, parent, &tx, memory).is_err(),
        "a second fresh counter cannot be started after any actual fact"
    );
    let points = originals_for_four_lanes(roots.height + 1, 1, memory, &objects, &mut io);
    for lane in RuntimeHistoryLaneV1::ALL {
        tx.append_fact(ExecutionFactRefV1 {
            kind: if lane == RuntimeHistoryLaneV1::Journal {
                ExecutionFactKindV1::Event
            } else {
                ExecutionFactKindV1::Result
            },
            id: points[lane.index()].location.original,
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
    let next = RuntimeHistoryLaneV1::ALL.map(|lane| {
        prepare_runtime_history_index_append_v1(
            parent_source.index(lane),
            points[lane.index()],
            &mut read,
            &mut write,
            &mut work(),
        )
        .unwrap()
    });
    let mut manifest = parent_source.manifest().clone();
    manifest.previous_view = Some(parent_source.occurrence().location().original);
    for lane in RuntimeHistoryLaneV1::ALL {
        manifest.lanes[lane.index()] = next[lane.index()].window().clone();
    }
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
    read.root = core.roots().state_root(StateNamespaceV1::Allocator);
    let mut consume_work = consumer_work();
    let consumed = read_candidate_runtime_history_source_v1(
        &RuntimeHistoryCandidateReadRequestV1 {
            candidate: &candidate,
            core: &core,
            originals: &originals,
            manifest: &manifest,
            selected_ordinal: 5,
            parent: Some(parent_source),
            prefix: None,
        },
        &mut objects.original_reader(&mut io),
        &mut read,
        &mut write,
        &mut consume_work,
    )
    .unwrap();
    assert_eq!(
        consume_work.originals.facts_remaining, 0,
        "ordinary only reads four delta originals"
    );
    assert!(
        consume_work.index.cells_remaining > 1500,
        "ordinary index work remains bounded to its four appends"
    );
    let prepared = prepare_runtime_history_source_promotion_v1(
        &consumed.source,
        consumed.indexes,
        memory,
        || false,
    )
    .unwrap();
    let accepted = formal::verify_formal_qualified(&proof, authority, Some(parent), None).unwrap();
    let blocker = memory
        .reserve((112 << 20) - memory.used(), MemoryClass::Ordinary)
        .unwrap();
    assert!(memory.reserve(1, MemoryClass::Ordinary).is_err());
    let used = memory.used();
    let after = finish_prepared_runtime_history_source_promotion_v1(prepared, &accepted).unwrap();
    assert_eq!(memory.used(), used);
    drop(blocker);
    for lane in RuntimeHistoryLaneV1::ALL {
        assert_eq!(after.manifest().lanes[lane.index()].total, 2);
        assert_eq!(
            after.index(lane).lineage(),
            None,
            "next accepted baseline is independent of prior delta"
        );
    }
    assert!(!attack, "P2 oracle stays in its separate original target");
}

fn consumer_work() -> RuntimeHistoryCandidateReadWorkV1 {
    RuntimeHistoryCandidateReadWorkV1 {
        rows_remaining: 16,
        max_new_rows: 4,
        index: RuntimeHistoryIndexWorkV1 {
            cells_remaining: 2000,
            max_pending_node_bytes: 1 << 20,
        },
        originals: FactBudget {
            facts_remaining: 4,
            original_bytes_remaining: 16 << 20,
            max_fact_bytes: 1 << 20,
        },
        decode:
            oasis7_world_state::incremental_runtime_history_decode::RuntimeHistoryDecodeWorkV1 {
                structure: oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1 {
                    bytes_remaining: 16 << 20,
                    values_remaining: 100_000,
                    max_depth: 64,
                },
                canonical: canonical_work(),
                max_original_bytes: 1 << 20,
                max_heap_bytes: 32 << 20,
            },
        max_world_event_heap_bytes: 32 << 20,
    }
}
fn originals_for_four_lanes(
    height: u64,
    index: u64,
    memory: &MemoryBudget,
    objects: &IncrementalObjectStore<'_>,
    io: &mut ObjectIo<'_>,
) -> [RuntimeHistoryIndexRowV1; 4] {
    use oasis7_world_state::{
        DomainEvent, WorldEvent, WorldEventBody,
        consensus::{TickConsensusRecord, TickConsensusRejectionAuditEvent},
    };
    let sequence = index + 1;
    let snapshot = SnapshotRecord {
        snapshot_hash: format!("original snapshot {sequence}"),
        journal_len: sequence as usize,
        created_at: sequence,
        manifest_hash: format!("original manifest {sequence}"),
    };
    let event = WorldEvent {
        id: sequence,
        time: sequence,
        caused_by: Some(oasis7_world_state::CausedBy::Action(sequence)),
        body: WorldEventBody::Domain(DomainEvent::BodyAttributesRejected {
            agent_id: "original agent".into(),
            reason: format!("original reason {sequence}"),
        }),
    };
    let tick:TickConsensusRecord=serde_json::from_value(serde_json::json!({"block":{"header":{"epoch":0,"tick":sequence,"parent_hash":"original parent","events_hash":"events","state_root":"state","executor_version":"v","randomness_seed":"seed"},"ordered_action_ids":[sequence],"ordered_event_ids":[sequence],"event_count":1,"execution_digest":{"action_batch_hash":"actions","domain_events_hash":"domain","state_projection_hash":"projection"}},"certificate":{"block_hash":"block","consensus_height":sequence,"threshold":1,"authority_source":"actual","submission_role":"authority","signatures":{"original key":"actual signature"}}})).unwrap();
    let audit:TickConsensusRejectionAuditEvent=serde_json::from_value(serde_json::json!({"recorded_at_tick":sequence,"tick":sequence,"consensus_height":sequence,"attempted_source":"original","attempted_role":"propagation","existing_source":null,"existing_role":null,"reason":"original reason"})).unwrap();
    let originals = [
        RuntimeHistoryOriginalRefV1::Snapshot(&snapshot),
        RuntimeHistoryOriginalRefV1::Event(&event),
        RuntimeHistoryOriginalRefV1::Tick(&tick),
        RuntimeHistoryOriginalRefV1::RejectionAudit(&audit),
    ];
    originals
        .into_iter()
        .enumerate()
        .map(|(lane, original)| {
            let mut bytes = Vec::new();
            with_runtime_history_original_v1(
                original,
                memory,
                &mut canonical_work(),
                &mut || false,
                1 << 20,
                |producer| {
                    producer.persist_to(
                        &mut |stream: &mut dyn oasis7_runtime::AgentEventOriginalStreamV1,
                              _: &mut dyn FnMut() -> bool| {
                            stream.write_to(&mut bytes)
                        },
                    )
                },
            )
            .unwrap();
            put(objects, &bytes, io);
            RuntimeHistoryIndexRowV1 {
                location: FactLocationV1 {
                    height,
                    ordinal: lane as u64 + 1,
                    original: *blake3::hash(&bytes).as_bytes(),
                },
                encoded_bytes: bytes.len() as u64,
            }
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap()
}
