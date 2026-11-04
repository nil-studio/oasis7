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
use oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use oasis7_world_state::incremental_runtime_history_original::*;
use oasis7_world_state::{incremental_cognition_history::*, incremental_world_result_router::*};
use oasis7_world_state::{
    incremental_cognition_history_cbor::CognitionHistoryCborWorkV1,
    incremental_runtime_history_decode::RuntimeHistoryDecodeWorkV1,
};
use oasis7_world_state::{
    incremental_field_member::*, incremental_runtime_history_index::*,
    incremental_runtime_history_manifest::*, snapshot::SnapshotRecord,
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
// Actual signatures and checked original bytes. These tests exercise Result
// closure only; they do not grant complete World execution or HistoryReady.
#[test]
fn actual_signed_genesis_result_router_requires_unique_both_views_and_closed_profiles() {
    for mode in 0..7 {
        let compression = if mode == 1 {
            PageCompression::Zstd
        } else {
            PageCompression::Raw
        };
        let directory = Directory::new();
        let path = directory.0.join("router");
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let _fixture = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
        let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
        let s = incremental::setup_with_memory(memory.clone());
        let cancelled = AtomicBool::new(false);
        let mut physical_io = physical();
        let mut logical_io = logical();
        let objects = IncrementalObjectStore::create(&store, &mut physical_io, &cancelled).unwrap();
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
                        &mut |stream: &mut dyn oasis7_world_state::incremental_original_producer::AgentEventOriginalStreamV1,
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
                count,
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
        let runtime_id = *blake3::hash(&bytes).as_bytes();
        if mode != 6 {
            genesis.push(ExecutionFactRefV1 {
                kind: ExecutionFactKindV1::Result,
                id: runtime_id,
            });
        }
        let cog_view = CognitionHistoryViewV1 {
            roots: s.roots.digest().unwrap(),
            enabled: false,
            revision: 0,
            previous_view: None,
            heads: std::array::from_fn(|_| CognitionHistoryHeadV1 {
                sequence: 0,
                digest: None,
            }),
            indexes: std::array::from_fn(|_| CognitionHistoryIndexRefV1 {
                present: false,
                records: 0,
                root: StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap(),
            }),
        };
        let cog_bytes = cognition_history_view_v3_bytes(
            &cog_view,
            &CognitionHistoryDerivedRefsV2 {
                caches: [None; 3],
                lifecycle: None,
            },
            &CognitionHistorySourceSummaryV3 {
                mode: CognitionHistorySourceModeV3::Bootstrap,
                cache_state_digests: [None; 3],
                last: std::array::from_fn(|_| None),
                lifecycle_keys: 0,
                lifecycle_journal_records: 0,
            },
            &memory,
            || false,
        )
        .unwrap();
        put(&objects, &cog_bytes[..], &mut io);
        let cognition_id = *blake3::hash(&cog_bytes[..]).as_bytes();
        if mode != 4 {
            genesis.push(ExecutionFactRefV1 {
                kind: ExecutionFactKindV1::Result,
                id: cognition_id,
            });
        }
        if mode == 2 {
            let unknown = b"unknown actual Result original";
            put(&objects, unknown, &mut io);
            genesis.push(ExecutionFactRefV1 {
                kind: ExecutionFactKindV1::Result,
                id: *blake3::hash(unknown).as_bytes(),
            });
        }
        if mode == 3 {
            genesis.push(ExecutionFactRefV1 {
                kind: ExecutionFactKindV1::Result,
                id: cognition_id,
            });
        }
        if mode == 5 {
            genesis.push(ExecutionFactRefV1 {
                kind: ExecutionFactKindV1::Result,
                id: runtime_id,
            });
        }
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

        let routed = route_checked_world_results_v1(
            &WorldResultRouterRequestV1 {
                candidate: &candidate,
                core: &core,
                originals: &originals,
            },
            &mut objects.original_reader(&mut io),
            &memory,
            &mut router_work(),
            || false,
        );
        if mode >= 2 {
            assert!(
                routed.is_err(),
                "actual signed Result set mode={mode} must be rejected"
            );
            continue;
        }
        let routed = routed.unwrap();
        assert_eq!(routed.rows().len(), 3);
        assert_eq!(
            routed.cognition().profile(),
            WorldResultProfileV1::CognitionHistoryV3
        );
        assert_eq!(
            routed.runtime_history().profile(),
            WorldResultProfileV1::RuntimeHistoryV1
        );
        assert_eq!(
            routed
                .rows()
                .iter()
                .filter(|row| row.profile() == WorldResultProfileV1::SnapshotV1)
                .count(),
            1
        );
        for row in routed.rows() {
            assert_eq!(
                row.occurrence().candidate_record_id(),
                core.candidate_record_id()
            );
            assert_eq!(row.occurrence().roots_id(), s.roots.digest().unwrap());
        }
    }
}
fn router_work() -> WorldResultRouterWorkV1 {
    let structure = || CborPreflightWorkV1 {
        bytes_remaining: 16 << 20,
        values_remaining: 100000,
        max_depth: 64,
    };
    WorldResultRouterWorkV1 {
        facts: facts(),
        envelope: structure(),
        runtime: RuntimeHistoryDecodeWorkV1 {
            structure: structure(),
            canonical: canonical_work(),
            max_original_bytes: 4 << 20,
            max_heap_bytes: 8 << 20,
        },
        cognition: CognitionHistoryCborWorkV1 {
            bytes_remaining: 16 << 20,
            nodes_remaining: 100000,
            source_reads_remaining: 100000,
            key_bytes_remaining: 1 << 20,
            max_depth: 64,
            max_key_bytes: 4096,
        },
    }
}
