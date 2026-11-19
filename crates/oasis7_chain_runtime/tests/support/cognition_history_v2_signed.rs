//! Real signatures and cold cache dependencies. Lifecycle remains an explicit
//! typed empty reference here; this fixture does not grant HistoryReady.
use super::signed::{core_budget, facts, put};
use super::*;
use oasis7_chain_runtime::incremental_cognition_history_store::persist_cognition_history_view_result_v2;
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
fn accepted_v2_retains_cache_references_through_cold_normal_continuation() {
    signed_v2(false);
}
#[test]
fn unrelated_accepted_predecessor_cannot_inherit_empty_lifecycle() {
    signed_v2(true);
}
pub(super) fn signed_v2(adversarial: bool) {
    signed_v2_source_mode(adversarial, false);
}
pub(super) fn signed_v2_source_mode(adversarial: bool, corrupt_source: bool) {
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
            let fact = persist_cognition_history_view_result_v2(
                &s.roots,
                &view,
                &refs,
                &memory,
                &objects,
                &mut io,
                || false,
            )
            .unwrap();
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
            let accepted =
                formal::verify_formal_qualified(&proof, &s.authority, None, None).unwrap();
            let selected =
                promote_cognition_history_view_v1(&checked, &accepted, &memory, || false).unwrap();
            assert_eq!(selected.derived(), Some(&refs));
            if adversarial {
                // Genuine separately accepted sibling: identical authenticated
                // IWS body/roots, but a distinct signed Formal record identity.
                let mut other_record = proof.decision.record.clone();
                other_record.projection_root = hex::encode([91; 32]);
                let other_proof = fixture::proof(
                    other_record,
                    &s.authority,
                    formal::FormalCompletionBranchV1::Normal,
                    None,
                    None,
                    4,
                    5,
                    6,
                );
                let other = formal::verify_formal_qualified(&other_proof, &s.authority, None, None)
                    .unwrap();
                assert_ne!(other.head().record_id, accepted.head().record_id);
                normal::continue_view(
                    normal::Source {
                        memory: &memory,
                        config: &s.config,
                        roots: &s.roots,
                        authority: &s.authority,
                        reuse: None,
                        alternate_parent: Some(&other),
                    },
                    (&history.body, &history.prefix),
                    &catalog.after,
                    (&accepted, &selected),
                    &store,
                    &objects,
                    &mut io,
                );
                return;
            }

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
        let mut no_edges = 0;
        assert!(matches!(
            visit_cognition_history_direct_dependencies_v2(
                &selected,
                &memory,
                &mut no_edges,
                |_| -> Result<(), ValueTreeError> { panic!("edge quota checked before visitor") },
                || false
            ),
            Err(CognitionHistoryDependencyVisitErrorV2::Value(
                ValueTreeError::ResourceLimited(_)
            ))
        ));
        assert!(
            bind_persisted_cognition_head_caches_v2(
                std::array::from_fn(|_| None),
                &selected,
                &binding,
                &memory,
                &mut work(),
                || false
            )
            .is_err()
        );
        let mut edges = 8;
        let mut caches = std::array::from_fn(|_| None);
        let mut originals = 0;
        let mut lifecycle = 0;
        let mut response_index = None;
        visit_cognition_history_direct_dependencies_v2(
            &selected,
            &memory,
            &mut edges,
            |edge| -> Result<(), ValueTreeError> {
                match edge {
                    CognitionHistoryDirectDependencyV2::HeadCache(kind, id) => {
                        let i = match kind {
                            CognitionHeadHashV1::Journal => 0,
                            CognitionHeadHashV1::Economy => 1,
                            CognitionHeadHashV1::Provision => 2,
                        };
                        caches[i] = Some(recover_cognition_head_cache_v1(
                            id,
                            &mut read.values,
                            &mut work(),
                            || false,
                        )?);
                    }
                    CognitionHistoryDirectDependencyV2::Index(field, _) => {
                        read.root = &selected.manifest().index_ref(field).root;
                        let proof = qualify_cognition_history_index_v1(
                            field,
                            selected.manifest().index_ref(field),
                            &mut read,
                            &mut CognitionHistoryIndexWorkV1 {
                                cells_remaining: 64,
                                max_pending_node_bytes: 1 << 20,
                            },
                            |_, _| {
                                originals += 1;
                                Ok(())
                            },
                        )
                        .map_err(|_| ValueTreeError::Invalid("v2 index GC closure"))?;
                        if field == CognitionHistoryFieldV1::Responses {
                            response_index = Some(proof);
                        }
                    }
                    CognitionHistoryDirectDependencyV2::Lifecycle(root) => {
                        assert_eq!(
                            *root,
                            StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap()
                        );
                        lifecycle += 1;
                    }
                }
                Ok(())
            },
            || false,
        )
        .unwrap();
        assert_eq!((originals, lifecycle), (0, 1));
        let bound = bind_persisted_cognition_head_caches_v2(
            caches,
            &selected,
            &binding,
            &memory,
            &mut work(),
            || false,
        )
        .unwrap();
        assert_eq!(bound.roots_id(), roots.digest().unwrap());
        drop(bound);
        drop(state);
        let view =
            readmit_cognition_history_view_ordinary_v1(&selected, &head, &roots, &memory, || false)
                .unwrap();
        let current_ordinary = readmit_cognition_history_current_ordinary_v1(
            &binding,
            CognitionHistoryCurrentSourceV1::Enabled(&current),
            &view,
            &roots,
            &memory,
            || false,
        )
        .unwrap();
        let index = readmit_cognition_history_index_ordinary_v1(
            response_index.as_ref().unwrap(),
            &view,
            &memory,
            || false,
        )
        .unwrap();
        drop(binding);
        drop(current);
        drop(selected);
        drop(response_index);
        let ready = readmission::OrdinaryFixtureProofs {
            view,
            current: current_ordinary,
            index: Some(index),
        };
        io.class = MemoryClass::Ordinary;
        let advanced = normal::continue_from_cold(
            normal::Source {
                alternate_parent: None,
                memory: &memory,
                config: &config,
                roots: &roots,
                authority: &authority,
                reuse: Some(&ready),
            },
            &head,
            &store,
            &objects,
            &mut io,
        )
        .unwrap();
        let state = store
            .state_session(super::physical(), MemoryClass::Ordinary)
            .unwrap();
        let after = recover_selected_cognition_history_view_v1(
            &FactRequestV1 {
                kind: ExecutionFactKindV1::Result,
                head: &advanced.0,
                roots: &advanced.1,
                location: oasis7_formal_protocol::incremental_material::FactLocationV1 {
                    height: advanced.1.height,
                    ordinal: 2,
                    original: advanced.2,
                },
            },
            &mut FactReadContextV1 {
                originals: &mut objects.original_reader(&mut io),
                catalog_pages: &state,
                memory: &memory,
                class: MemoryClass::Ordinary,
                core_budget: &mut core_budget(),
                fact_budget: &mut facts(),
                cancelled: || false,
            },
        )
        .unwrap();
        assert_eq!(after.derived(), ready.view.derived());
        assert_eq!(after.manifest().indexes[1].records, 1);
        super::journal_normal::append_two_events(
            super::journal_normal::JournalFrame {
                source: normal::Source {
                    alternate_parent: None,
                    memory: &memory,
                    config: &config,
                    roots: &advanced.1,
                    authority: &authority,
                    reuse: None,
                },
                head: &advanced.0,
                view: &after,
                complete: None,
            },
            &store,
            &objects,
            &mut io,
            corrupt_source,
        );
    }
}
