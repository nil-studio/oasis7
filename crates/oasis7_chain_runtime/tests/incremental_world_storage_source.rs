#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget,
    objects::{IncrementalObjectStore, ObjectIo},
};
use oasis7_formal_protocol::{
    controlled_authority::formal::{self, test_fixtures::incremental},
    incremental_execution::{ExecutionFactKindV1, ExecutionFactRefV1, RootSetV1},
    incremental_material::*,
    incremental_state::*,
    incremental_value::*,
};
use oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use oasis7_world_state::{incremental_cognition_history::*, incremental_world_result_router::*};
use oasis7_world_state::{
    incremental_cognition_history_cbor::CognitionHistoryCborWorkV1,
    incremental_runtime_history_decode::RuntimeHistoryDecodeWorkV1,
};
use oasis7_world_state::{
    incremental_field_member::*, incremental_runtime_history_index::*,
    incremental_runtime_history_manifest::*,
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

use oasis7_formal_protocol::incremental_history::GenesisConfigV1;
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_runtime::{World, incremental_genesis::RuntimeGenesisWorkV1};
use oasis7_world_state::incremental_semantic_key::SemanticKeyBudget;
use oasis7_world_state::{
    WorldState, incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_history_source::*, incremental_world_current_complete::*,
};
#[path = "support/world_storage_cognition.rs"]
mod cognition;
#[path = "support/world_storage_current_work.rs"]
mod current_work;
#[path = "support/world_storage_genesis_work.rs"]
mod genesis_work;
#[path = "support/world_storage_physical_genesis.rs"]
mod physical_genesis;
#[path = "support/world_storage_scratch.rs"]
mod scratch;

// An actual empty fresh World has no mailbox references. This reader rejects
// every unexpected request and records it, rather than minting original proofs.
struct NoMailbox(usize);
impl<O: OriginalObjectStreamReader>
    oasis7_world_state::incremental_world_raw_complete::MailboxDomainEventOriginalReaderV1<O>
    for NoMailbox
{
    fn checked_domain_event(
        &mut self,
        _: oasis7_world_state::incremental_agent_subtree::AgentMailboxReferenceV1<'_>,
        _: &mut O,
        _: &MemoryBudget,
        _: MemoryClass,
        _: &mut oasis7_world_state::incremental_world_raw_complete::DomainEventOriginalWorkV1,
        _: &mut dyn FnMut() -> bool,
    ) -> Result<
        std::sync::Arc<
            ReservedValue<
                oasis7_world_state::incremental_world_raw_complete::CheckedDomainEventOriginalV1,
            >,
        >,
        oasis7_world_state::incremental_world_raw_complete::DomainEventOriginalErrorV1<O::Error>,
    > {
        self.0 += 1;
        Err(
            oasis7_world_state::incremental_world_raw_complete::DomainEventOriginalErrorV1::Invalid(
                "unexpected mailbox in empty fresh World",
            ),
        )
    }
}
fn state_error(_: oasis7_distfs::incremental_page_store::PageStoreError) -> ValueTreeError {
    ValueTreeError::Invalid("actual state page read")
}

#[test]
fn real_default_world_genesis_qualifies_complete_storage_sources() {
    for mode in 0..3 {
        let directory = Directory::new();
        let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
        let _fixture = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
        let store = IncrementalPageStore::create(
            &directory.0.join("complete"),
            memory.clone(),
            PageCompression::Raw,
        )
        .unwrap();
        let cancelled = AtomicBool::new(false);
        let world = World::new_with_state(WorldState::default());
        let (native, native_objects) = incremental::native_fixture();
        let native_bytes = native.encode().unwrap();
        let config = GenesisConfigV1 {
            world_id: "complete-storage-source".into(),
            format_profile:
                oasis7_world_state::incremental_world_format::complete_world_format_profile_v1(),
            initial_content: *blake3::hash(b"actual empty world initial content").as_bytes(),
            activation_config: *blake3::hash(b"actual test activation config").as_bytes(),
            execution_environment: *blake3::hash(&native_bytes).as_bytes(),
        };
        let (physical_roots, summary) =
            physical_genesis::prepare(&world, &store, &config, &memory, &cancelled);
        assert_eq!(summary.typed.world_state.native_fields, 97);
        assert_eq!(summary.typed.runtime_fields, 46);
        assert_eq!(summary.typed.cognition_fields, 9);
        let mut object_physical = physical();
        let mut object_logical = logical();
        let objects =
            IncrementalObjectStore::create(&store, &mut object_physical, &cancelled).unwrap();
        let mut object_io = ObjectIo {
            physical: &mut object_physical,
            logical: &mut object_logical,
            class: MemoryClass::Ordinary,
            cancelled: &cancelled,
        };
        for bytes in native_objects.iter() {
            put(&objects, bytes, &mut object_io);
        }
        put(&objects, &native_bytes, &mut object_io);
        // Distinct actual persisted artifact originals. Execution legality and native
        // code execution are deliberately separate from this source conjunction.
        let artifact_bytes: [&[u8]; 4] = [
            b"actual manifest original",
            b"actual wasm original",
            b"actual rules original",
            b"actual policy original",
        ];
        for bytes in artifact_bytes {
            put(&objects, bytes, &mut object_io);
        }
        let refs = || incremental::FixtureExecutionArtifactRefsV1 {
            manifest: *blake3::hash(artifact_bytes[0]).as_bytes(),
            wasm: *blake3::hash(artifact_bytes[1]).as_bytes(),
            rules: *blake3::hash(artifact_bytes[2]).as_bytes(),
            policy: *blake3::hash(artifact_bytes[3]).as_bytes(),
        };
        let before = incremental::setup_with_config_and_state(
            memory.clone(),
            config.clone(),
            physical_roots.state.clone(),
            refs(),
        );
        let mut writer = store.value_session(physical());
        let mut value_io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
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
        let borrowed = world.borrow_iws_runtime_history_v1();
        assert!(borrowed.snapshot_catalog.records.is_empty());
        assert!(borrowed.journal.events.is_empty());
        assert!(borrowed.ticks.is_empty());
        assert!(borrowed.rejection_audit.is_empty());
        assert_eq!(borrowed.archived_tick_count, 0);
        let lanes = RuntimeHistoryLaneV1::ALL.map(|lane| RuntimeHistoryWindowV1 {
            root: prepare_runtime_history_index_v1(
                lane,
                0,
                std::iter::empty::<Result<RuntimeHistoryIndexRowV1, ValueTreeError>>(),
                &mut write,
                &mut work(),
            )
            .unwrap(),
            total: 0,
            start: 0,
            count: 0,
        });
        let runtime_manifest = RuntimeHistoryManifestV1 {
            roots: before.roots.digest().unwrap(),
            previous_view: None,
            lanes,
            max_snapshots: borrowed.snapshot_catalog.retention.max_snapshots as u64,
            archived_tick_count: borrowed.archived_tick_count,
        };
        let runtime_bytes = runtime_manifest.canonical_bytes().unwrap();
        let runtime_id = *blake3::hash(&runtime_bytes).as_bytes();
        put(&objects, &runtime_bytes, &mut object_io);
        let (cog_view, cog_refs, cog_bytes) = cognition::prepare(&before.roots, &store, &memory);
        let cog_id = *blake3::hash(&cog_bytes).as_bytes();
        put(&objects, &cog_bytes, &mut object_io);
        let mut extras = vec![ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Result,
            id: runtime_id,
        }];
        if mode != 1 {
            extras.push(ExecutionFactRefV1 {
                kind: ExecutionFactKindV1::Result,
                id: cog_id,
            });
        }
        let s = incremental::setup_with_config_state_and_facts(
            memory.clone(),
            config.clone(),
            physical_roots.state.clone(),
            refs(),
            &extras,
        );
        assert_eq!(s.roots.digest().unwrap(), before.roots.digest().unwrap());
        let (blobs, catalog) = incremental::core_objects(&s, &s.history);
        for blob in blobs.iter() {
            put(&objects, blob.as_slice(), &mut object_io);
        }
        {
            let mut poll = || false;
            let mut pages = store
                .buffered_page_writer(object_io.physical, &cancelled, &mut poll)
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
        let candidate = formal::verify_formal_candidate(
            &formal::SignedCanonicalCandidateV1 {
                record: s.proof.decision.record.clone(),
                writer_signature_hex: s.proof.decision.writer_signature_hex.clone(),
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
            &mut objects.original_reader(&mut object_io),
            &state,
            &memory,
            &mut core_budget(),
            || false,
        )
        .unwrap();
        let originals = verify_fact_originals(
            &core,
            &mut objects.original_reader(&mut object_io),
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
            &mut objects.original_reader(&mut object_io),
            &memory,
            &mut router_work(),
            || false,
        );
        if mode == 1 {
            assert!(routed.is_err(), "missing actual Cog Result must fail");
            continue;
        }
        let routed = routed.unwrap();
        let values = store.value_session(physical());
        let mut read_io = logical();
        let mut read_keys = SemanticKeyBudget {
            bytes_remaining: 64 << 20,
        };
        let mut ctx = WorldFieldReadContextV1 {
            root: s.roots.state_root(StateNamespaceV1::Allocator),
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut read_io,
            },
            key_work: &mut read_keys,
            semantic_bytes: 4096,
            record_bytes: 9 << 20,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let mut mailbox = NoMailbox(0);
        let current=qualify_world_current_state_v1(&s.roots,
   oasis7_world_state::incremental_cognition_current_complete_v3::CognitionCurrentPresenceV3::Enabled,
   &mut ctx,&mut current_work::current_work(),&scratch::Scratch::new(&memory,4096),
   &mut objects.original_reader(&mut object_io),&mut mailbox).unwrap();
        assert_eq!(mailbox.0, 0);
        let indexes = RuntimeHistoryLaneV1::ALL.map(|lane| {
            qualify_runtime_history_index_v1(
                lane,
                &runtime_manifest.lanes[lane.index()],
                &mut ctx,
                &mut work(),
            )
            .unwrap()
        });
        let runtime = check_candidate_runtime_history_source_v1(
            &candidate,
            &core,
            &originals,
            &runtime_manifest,
            routed.runtime_history().occurrence().location().ordinal,
            &indexes,
            None,
            &[],
            &mut ctx,
            &mut RuntimeHistorySourceWorkV1 {
                rows_remaining: 64,
                index: work(),
            },
        )
        .unwrap();
        let request = CognitionHistorySelectionRequestV1 {
            candidate: &candidate,
            core: &core,
            originals: &originals,
        };
        let selected = check_candidate_cognition_history_view_v1(
            &request,
            &mut objects.original_reader(&mut object_io),
            &memory,
            &mut facts(),
            || false,
        )
        .unwrap();
        let cognition = cognition::qualify(
            &request, selected, &cog_view, &cog_refs, &current, &s.roots, &mut ctx, &memory,
        );
        let other;
        let aggregation_core = if mode == 2 {
            let mut other_facts = extras.clone();
            other_facts.reverse();
            let other_setup = incremental::setup_with_config_state_and_facts(
                memory.clone(),
                config.clone(),
                physical_roots.state.clone(),
                refs(),
                &other_facts,
            );
            let (other_blobs, other_catalog) =
                incremental::core_objects(&other_setup, &other_setup.history);
            for blob in other_blobs.iter() {
                put(&objects, blob.as_slice(), &mut object_io);
            }
            {
                let mut poll = || false;
                let mut pages = store
                    .buffered_page_writer(object_io.physical, &cancelled, &mut poll)
                    .unwrap();
                for node in &other_catalog.new_nodes {
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
            let other_candidate = formal::verify_formal_candidate(
                &formal::SignedCanonicalCandidateV1 {
                    record: other_setup.proof.decision.record.clone(),
                    writer_signature_hex: other_setup.proof.decision.writer_signature_hex.clone(),
                },
                None,
                &other_setup.authority,
                None,
                None,
                formal::FormalCompletionBranchV1::Normal,
            )
            .unwrap();
            other = verify_commit_core(
                &CoreRequest {
                    candidate: &other_candidate,
                    original_authority: &other_setup.authority,
                    parent: None,
                    max_pending_node_bytes: 4 << 20,
                },
                &mut objects.original_reader(&mut object_io),
                &state,
                &memory,
                &mut core_budget(),
                || false,
            )
            .unwrap();
            let _other_originals = verify_fact_originals(
                &other,
                &mut objects.original_reader(&mut object_io),
                &memory,
                &mut facts(),
                || false,
            )
            .unwrap();
            assert_ne!(other.candidate_record_id(), core.candidate_record_id());
            &other
        } else {
            &core
        };
        let source=oasis7_world_state::incremental_world_storage_source::qualify_checked_world_storage_source_v1(
   aggregation_core,current,cognition,runtime,routed,&memory,||false);
        if mode == 2 {
            assert!(
                source.is_err(),
                "same roots from another genuinely signed body must fail"
            );
        } else {
            let source = source.unwrap();
            source.current().validate_binding(&s.roots).unwrap();
            assert_eq!(source.results().rows().len(), 2);
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
