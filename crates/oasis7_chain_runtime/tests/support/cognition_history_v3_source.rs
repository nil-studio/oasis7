//! Real signatures and a complete empty GEN Cognition source conjunction.
//! Runtime History and physical World readiness remain separate obligations.
use super::signed::{core_budget, facts, put};
use super::*;

use oasis7_formal_protocol::{
    controlled_authority::{
        formal::{
            self,
            test_fixtures::{self as fixture, incremental},
        },
        replicated_protocol::ArtifactRole,
    },
    incremental_execution::ExecutionFactKindV1,
    incremental_history::{CatalogRootV1, HeaderPreparation},
    incremental_material::{
        CoreRequest, FactReadContextV1, FactRequestV1, verify_commit_core, verify_fact_originals,
    },
    incremental_record_root::RecordRootRefV1,
    incremental_state::{StateNamespaceV1, StateRootV1},
};
use oasis7_world_state::incremental_cognition_history_hash_outboard::*;
fn work() -> CognitionHeadHashWorkV1 {
    CognitionHeadHashWorkV1 {
        input_bytes_remaining: 128 << 20,
        hash_nodes_remaining: 1000000,
    }
}
#[test]
fn actual_v3_enabled_genesis_recovers_source_in_fresh_pool() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("v2");
        let (head, roots, location, config, authority) = {
            let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
            let _fixture = memory.reserve(512 << 10, MemoryClass::Ordinary).unwrap();
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            let enabled = enabled::genesis(&store, &memory, false);
            let s = incremental::setup_with_state(memory.clone(), enabled.state);
            let cancelled = AtomicBool::new(false);
            let mut physical = physical();
            let mut logical = logical();
            let objects =
                IncrementalObjectStore::create(&store, &mut physical, &cancelled).unwrap();
            let mut io = ObjectIo {
                physical: &mut physical,
                logical: &mut logical,
                class: MemoryClass::Ordinary,
                cancelled: &cancelled,
            };
            let cache = CognitionHeadHashBuilderV1::new(&memory, MemoryClass::Ordinary, || false)
                .unwrap()
                .finish(|| false)
                .unwrap();
            let cache_id = persist_cognition_head_cache_v1(
                &cache,
                &mut store.value_session(super::physical()),
                &memory,
                &mut super::logical(),
                &mut work(),
                || false,
            )
            .unwrap();
            let view = CognitionHistoryViewV1 {
                roots: s.roots.digest().unwrap(),
                enabled: true,
                revision: 0,
                previous_view: None,
                heads: std::array::from_fn(|i| CognitionHistoryHeadV1 {
                    sequence: 0,
                    digest: enabled.heads[i],
                }),
                indexes: enabled.indexes,
            };
            let refs = CognitionHistoryDerivedRefsV2 {
                caches: [Some(cache_id), None, None],
                lifecycle: Some(
                    RecordRootRefV1::new(
                        lifecycle_schema(),
                        lifecycle_owner(),
                        StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap(),
                    )
                    .unwrap(),
                ),
            };
            let fact =
                persist_v3(&s.roots, &view, &refs, &memory, &objects, &mut io, || false).unwrap();
            let mut genesis = s.history.facts.facts.clone();
            genesis.push(fact);
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
                    let charge = memory
                        .reserve(node.bytes.capacity() + 128, MemoryClass::Ordinary)
                        .unwrap();
                    use oasis7_formal_protocol::incremental_value::ValueNodeWriter;
                    pages
                        .put_node(
                            *blake3::hash(&node.bytes).as_bytes(),
                            charge.retain(node.bytes.clone()),
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
                .state_session(super::physical(), MemoryClass::Ordinary)
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
            let checked = check_candidate_cognition_history_view_v1(
                &CognitionHistorySelectionRequestV1 {
                    candidate: &candidate,
                    core: &core,
                    originals: &originals,
                },
                &mut objects.original_reader(&mut io),
                &memory,
                &mut facts(),
                || false,
            )
            .unwrap();
            let values = store.value_session(super::physical());
            let mut value_io = super::logical();
            let mut key_work = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut read = WorldFieldReadContextV1 {
                root: &view.indexes[0].root,
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut value_io,
                },
                key_work: &mut key_work,
                semantic_bytes: 4096,
                record_bytes: 8192,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            };
            let index = qualify_cognition_history_index_v1(
                CognitionHistoryFieldV1::JournalEvents,
                &view.indexes[0],
                &mut read,
                &mut CognitionHistoryIndexWorkV1 {
                    cells_remaining: 64,
                    max_pending_node_bytes: 1 << 20,
                },
                |_, _| panic!("explicit empty GEN"),
            )
            .unwrap();
            let physical_cache =
                recover_cognition_head_cache_v1(cache_id, &mut read.values, &mut work(), || false)
                    .unwrap();
            let request = CognitionHistorySelectionRequestV1 {
                candidate: &candidate,
                core: &core,
                originals: &originals,
            };
            let checked_source = CandidateCognitionHeadSourceBuilderV3::genesis(
                &request,
                &checked,
                &index,
                &memory,
                || false,
            )
            .unwrap()
            .finish(&physical_cache, &mut work(), || false)
            .unwrap();
            let response_index = qualify_cognition_history_index_v1(
                CognitionHistoryFieldV1::Responses,
                &view.indexes[1],
                &mut read,
                &mut CognitionHistoryIndexWorkV1 {
                    cells_remaining: 64,
                    max_pending_node_bytes: 1 << 20,
                },
                |_, _| panic!("explicit empty GEN responses"),
            )
            .unwrap();
            let journal_source = CandidateCognitionHistoryIndexBuilderV3::new(
                &request,
                &checked,
                CognitionHistoryFieldV1::JournalEvents,
                Some(&index),
                None,
                &memory,
                || false,
            )
            .unwrap()
            .finish(|| false)
            .unwrap();
            let response_source = CandidateCognitionHistoryIndexBuilderV3::new(
                &request,
                &checked,
                CognitionHistoryFieldV1::Responses,
                Some(&response_index),
                None,
                &memory,
                || false,
            )
            .unwrap()
            .finish(|| false)
            .unwrap();
            let current =
                enabled::qualify_current(&s.roots, &store, &memory, MemoryClass::Ordinary);
            read.root = s.roots.state_root(StateNamespaceV1::Allocator);
            let checked_current = bind_candidate_cognition_history_current_v3(
                &request,
                &checked,
                &s.roots,
                CognitionHistoryCurrentSourceV1::Enabled(&current),
                &mut read,
            )
            .unwrap();
            let checked_lifecycle = oasis7_world_state::incremental_cognition_lifecycle_qualification_v2::CandidateCognitionLifecycleBuilderV3::new(
                &request, &checked, None, &memory, || false,
            ).unwrap().finish(|| false).unwrap();
            let checked_whole = qualify_checked_cognition_history_source_v3(
                CheckedCognitionHistoryComponentsV3 {
                    selected: checked,
                    current: checked_current,
                    indexes: [Some(journal_source), Some(response_source), None, None],
                    heads: [Some(checked_source), None, None],
                    lifecycle: Some(checked_lifecycle),
                    disabled: None,
                },
                &memory,
                || false,
            )
            .unwrap();
            let production_checked = super::source_reader::check(
                &request,
                CognitionHistoryCurrentSourceV1::Enabled(&current),
                None,
                &store,
                &objects,
                &mut io,
                &memory,
            );
            assert_eq!(
                production_checked.selected().manifest(),
                checked_whole.selected().manifest()
            );
            let pending = super::source_reader::prepare(&production_checked, &memory);
            let accepted =
                formal::verify_formal_qualified(&proof, &s.authority, None, None).unwrap();
            let promoted_whole = promote_complete_cognition_history_source_v3(
                &checked_whole,
                &accepted,
                &memory,
                || false,
            )
            .unwrap();
            let production_promoted =
                super::source_reader::finish_without_admission(pending, &accepted, &memory);
            assert_eq!(
                production_promoted.selected().manifest(),
                promoted_whole.selected().manifest()
            );
            let selected = production_promoted.selected();
            assert_eq!(selected.derived(), Some(&refs));
            assert!(promoted_whole.lifecycle().is_some());
            assert_eq!(
                promoted_whole
                    .indexes()
                    .iter()
                    .filter(|slot| slot.is_some())
                    .count(),
                2
            );
            assert!(promoted_whole.heads()[0].is_some());

            (
                accepted,
                s.roots.clone(),
                selected.occurrence().location(),
                s.config.clone(),
                s.authority.clone(),
            )
        };
        // No producer DTO, cache, selected proof or old memory pool survives.
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Recovery)
                .unwrap();
        let cancelled = AtomicBool::new(false);
        let mut physical = physical();
        let mut logical = logical();
        let objects =
            IncrementalObjectStore::open(&store, MemoryClass::Recovery, &mut physical, &cancelled)
                .unwrap();
        let state = store
            .state_session(super::physical(), MemoryClass::Recovery)
            .unwrap();
        let values = store.value_session(super::physical());
        let mut io = ObjectIo {
            physical: &mut physical,
            logical: &mut logical,
            class: MemoryClass::Recovery,
            cancelled: &cancelled,
        };
        let selected = recover_selected_cognition_history_view_v1(
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
        let current = enabled::qualify_current(&roots, &store, &memory, MemoryClass::Recovery);
        let mut read_io = super::logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut read = WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Allocator),
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut read_io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let binding = bind_cognition_history_current_v1(
            &selected,
            &roots,
            CognitionHistoryCurrentSourceV1::Enabled(&current),
            &mut read,
        )
        .unwrap();

        assert!(selected.source_v3().is_some());
        let cache = recover_cognition_head_cache_v1(
            selected.derived().unwrap().caches[0].unwrap(),
            &mut read.values,
            &mut work(),
            || false,
        )
        .unwrap();
        let caches = bind_persisted_cognition_head_caches_v2(
            [Some(cache), None, None],
            &selected,
            &binding,
            &memory,
            &mut work(),
            || false,
        )
        .unwrap();
        let source = recover_attested_cognition_head_source_v3(
            CognitionHistoryFieldV1::JournalEvents,
            &selected,
            &binding,
            &caches,
            None,
            &memory,
            &mut work(),
            || false,
        )
        .unwrap();
        let journal_index = recover_attested_cognition_history_index_v3(
            CognitionHistoryFieldV1::JournalEvents,
            &selected,
            &binding,
            &memory,
            || false,
        )
        .unwrap();
        let response_index = recover_attested_cognition_history_index_v3(
            CognitionHistoryFieldV1::Responses,
            &selected,
            &binding,
            &memory,
            || false,
        )
        .unwrap();
        let lifecycle = oasis7_world_state::incremental_cognition_lifecycle_qualification_v2::recover_attested_cognition_lifecycle_v3(&selected,&binding,&memory,|| false).unwrap();
        let ordinary_selected =
            readmit_cognition_history_view_ordinary_v1(&selected, &head, &roots, &memory, || false)
                .unwrap();
        let ordinary_current = readmit_cognition_history_current_ordinary_v1(
            &binding,
            CognitionHistoryCurrentSourceV1::Enabled(&current),
            &ordinary_selected,
            &roots,
            &memory,
            || false,
        )
        .unwrap();
        let ordinary_state = store
            .state_session(super::physical(), MemoryClass::Ordinary)
            .unwrap();
        let mut ordinary_read = WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Allocator),
            state: &ordinary_state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut read_io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || ordinary_state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let ordinary_binding = bind_cognition_history_current_v1(
            &ordinary_selected,
            &roots,
            ordinary_current.current(),
            &mut ordinary_read,
        )
        .unwrap();
        let recovered = recover_complete_cognition_history_source_v3(
            RecoveredCognitionHistoryComponentsV3 {
                selected,
                current: binding,
                indexes: [Some(journal_index), Some(response_index), None, None],
                heads: [Some(source), None, None],
                lifecycle: Some(lifecycle),
                disabled: None,
            },
            &memory,
            &mut work(),
            || false,
        )
        .unwrap();
        assert!(recovered.attested_lifecycle().is_some());
        let ordinary = readmit_complete_cognition_history_source_ordinary_v3(
            &recovered,
            ordinary_selected,
            ordinary_binding,
            &memory,
            &mut work(),
            || false,
        )
        .unwrap();
        drop(recovered);
        drop(caches);
        drop(current);
        drop(ordinary_current);
        assert!(ordinary.uses_budget(&memory));
        assert!(ordinary.attested_lifecycle().is_some());
        assert_eq!(
            ordinary.indexes()[0].as_ref().unwrap().allocation_class(),
            MemoryClass::Ordinary
        );
        io.class = MemoryClass::Ordinary;
        super::journal_normal::append_two_events(
            super::journal_normal::JournalFrame {
                source: normal::Source {
                    memory: &memory,
                    config: &config,
                    roots: &roots,
                    authority: &authority,
                    alternate_parent: None,
                    reuse: None,
                },
                head: &head,
                view: ordinary.selected(),
                complete: Some(&ordinary),
            },
            &store,
            &objects,
            &mut io,
            false,
        );
    }
}

fn persist_v3(
    roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
    view: &CognitionHistoryViewV1,
    refs: &CognitionHistoryDerivedRefsV2,
    memory: &MemoryBudget,
    objects: &IncrementalObjectStore<'_>,
    io: &mut ObjectIo<'_>,
    mut cancelled: impl FnMut() -> bool,
) -> Result<oasis7_formal_protocol::incremental_execution::ExecutionFactRefV1, ValueTreeError> {
    assert_eq!(roots.digest().unwrap(), view.roots);
    let state = CognitionHeadHashBuilderV1::new(memory, MemoryClass::Ordinary, &mut cancelled)?
        .finish(&mut cancelled)?;
    struct Digest(blake3::Hasher);
    impl std::io::Write for Digest {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.update(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut digest = Digest(blake3::Hasher::new());
    state.write_canonical_cache_v1(&mut digest, &mut work(), &mut cancelled)?;
    let summary = CognitionHistorySourceSummaryV3 {
        mode: CognitionHistorySourceModeV3::Bootstrap,
        cache_state_digests: [Some(*digest.0.finalize().as_bytes()), None, None],
        last: [None, None, None],
        lifecycle_keys: 0,
        lifecycle_journal_records: 0,
    };
    persist_v3_summary(view, refs, &summary, memory, objects, io, cancelled)
}
pub(super) fn persist_v3_summary(
    view: &CognitionHistoryViewV1,
    refs: &CognitionHistoryDerivedRefsV2,
    summary: &CognitionHistorySourceSummaryV3,
    memory: &MemoryBudget,
    objects: &IncrementalObjectStore<'_>,
    io: &mut ObjectIo<'_>,
    mut cancelled: impl FnMut() -> bool,
) -> Result<oasis7_formal_protocol::incremental_execution::ExecutionFactRefV1, ValueTreeError> {
    let bytes = cognition_history_view_v3_bytes(view, refs, summary, memory, &mut cancelled)?;
    let id = *blake3::hash(bytes.as_slice()).as_bytes();
    put(objects, bytes.as_slice(), io);
    Ok(
        oasis7_formal_protocol::incremental_execution::ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Result,
            id,
        },
    )
}
