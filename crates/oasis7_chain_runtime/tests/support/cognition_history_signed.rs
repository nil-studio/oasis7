//! Actual protocol signatures and physical originals; the disabled cohort
//! exercises metadata selection, not semantic qualification of nonempty History.
use super::*;
use oasis7_chain_runtime::incremental_cognition_history_store::persist_cognition_history_view_result_v1;
use oasis7_distfs::incremental_page_store::PageStoreError;
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
    incremental_material::{
        CoreBudget, CoreRequest, FactBudget, FactReadContextV1, FactRequestV1,
        OriginalObjectReader, verify_commit_core, verify_fact_occurrence_v1, verify_fact_originals,
    },
    incremental_state::{StateNamespaceV1, StateRootV1},
    incremental_value::ValueNodeWriter,
};

fn disabled(roots: [u8; 32]) -> CognitionHistoryViewV1 {
    CognitionHistoryViewV1 {
        roots,
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
    }
}
pub(super) fn core_budget() -> CoreBudget {
    CoreBudget {
        objects_remaining: 64,
        original_bytes_remaining: 8 << 20,
        new_pages_remaining: 4096,
        merkle_reads_remaining: 100_000,
    }
}
pub(super) fn facts() -> FactBudget {
    FactBudget {
        facts_remaining: 16,
        original_bytes_remaining: 8 << 20,
        max_fact_bytes: 4 << 20,
    }
}
pub(super) fn put(objects: &IncrementalObjectStore<'_>, bytes: &[u8], io: &mut ObjectIo<'_>) {
    let mut input = bytes;
    objects
        .write_object(
            &mut input,
            bytes.len() as u64,
            Some(*blake3::hash(bytes).as_bytes()),
            io,
        )
        .unwrap();
}

#[test]
fn actual_signed_views_require_unique_result_and_cold_normal_continuation() {
    signed_modes(0..6);
}

#[test]
fn authenticated_index_point_joins_actual_original_and_accepted_result() {
    // The existing small enabled scenario, without repeating the unrelated
    // all-20-slots/64MiB readmission scenario.
    signed_modes(4..5);
}

fn signed_modes(modes: impl IntoIterator<Item = u8>) {
    for mode in modes {
        let directory = Directory::new();
        let path = directory.0.join("signed");
        // The selected formal head/RootSet are bounded external trust inputs,
        // not an old Snapshot, original DTO or a retained History proof.
        let anchor_memory = MemoryBudget::new(1 << 20, 0).unwrap();
        let _anchors = anchor_memory
            .reserve(128 << 10, MemoryClass::Ordinary)
            .unwrap();
        let accepted = {
            let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
            let _fixture = memory.reserve(512 << 10, MemoryClass::Ordinary).unwrap();
            let store =
                IncrementalPageStore::create(&path, memory.clone(), PageCompression::Raw).unwrap();
            let enabled = (mode >= 4).then(|| enabled::genesis(&store, &memory, mode == 5));
            let s = if let Some(enabled) = &enabled {
                incremental::setup_with_state(memory.clone(), enabled.state.clone())
            } else {
                incremental::setup_with_memory(memory.clone())
            };
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
            let mut view = disabled(if mode == 2 {
                [0x99; 32]
            } else {
                s.roots.digest().unwrap()
            });
            if let Some(enabled) = enabled {
                view.enabled = true;
                view.indexes = enabled.indexes;
                for (head, digest) in view.heads.iter_mut().zip(enabled.heads) {
                    head.digest = digest;
                }
            }
            let bytes = view.canonical_bytes(&memory, || false).unwrap();
            // The adversarial wrong-root original is deliberately published by
            // the lower-level store, not the production after-root consumer.
            if mode == 2 {
                assert!(matches!(
                    persist_cognition_history_view_result_v1(
                        &s.roots,
                        &view,
                        &memory,
                        &objects,
                        &mut io,
                        || false
                    ),
                    Err(ValueTreeError::Invalid(_))
                ));
                put(&objects, bytes.as_slice(), &mut io);
            } else {
                let published = persist_cognition_history_view_result_v1(
                    &s.roots,
                    &view,
                    &memory,
                    &objects,
                    &mut io,
                    || false,
                )
                .unwrap();
                assert_eq!(published.kind, ExecutionFactKindV1::Result);
                assert_eq!(published.id, *blake3::hash(bytes.as_slice()).as_bytes());
            }
            let id = *blake3::hash(bytes.as_slice()).as_bytes();
            let mut facts = s.history.facts.facts.clone();
            facts.push(ExecutionFactRefV1 {
                kind: if mode == 3 {
                    ExecutionFactKindV1::Event
                } else {
                    ExecutionFactKindV1::Result
                },
                id,
            });
            if mode == 1 {
                facts.push(ExecutionFactRefV1 {
                    kind: ExecutionFactKindV1::Result,
                    id,
                });
            }
            let history = HeaderPreparation {
                config: &s.config,
                catalog: &CatalogRootV1::empty(s.config.scope().unwrap()).unwrap(),
                roots: &s.roots,
                parent: None,
                parent_history: None,
                transition: None,
                genesis_facts: &facts,
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
                    let hash = *blake3::hash(&node.bytes).as_bytes();
                    assert_eq!(blake3::Hash::from(hash).to_hex().as_str(), node.digest);
                    let charge = memory
                        .reserve(node.bytes.capacity() + 128, MemoryClass::Ordinary)
                        .unwrap();
                    pages
                        .put_node(hash, charge.retain(node.bytes.clone()))
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
            // This independent generic gate really succeeds even in all three
            // typed-invalid cases. No forged private core or occurrence token.
            let originals = verify_fact_originals(
                &core,
                &mut objects.original_reader(&mut io),
                &memory,
                &mut self::facts(),
                || false,
            )
            .unwrap();
            let result = check_candidate_cognition_history_view_v1(
                &CognitionHistorySelectionRequestV1 {
                    candidate: &candidate,
                    core: &core,
                    originals: &originals,
                },
                &mut objects.original_reader(&mut io),
                &memory,
                &mut self::facts(),
                || false,
            );
            if (1..=3).contains(&mode) {
                assert!(matches!(
                    result,
                    Err(CognitionHistorySelectionErrorV1::Value(
                        ValueTreeError::Invalid(_)
                    ))
                ));
                None
            } else {
                let selected = result.unwrap();
                assert_eq!(selected.manifest().enabled, mode >= 4);
                assert_eq!(selected.manifest().roots, s.roots.digest().unwrap());
                assert_eq!(selected.occurrence().location().ordinal, 1);
                assert_eq!(selected.occurrence().location().original, id);
                let accepted =
                    formal::verify_formal_qualified(&proof, &s.authority, None, None).unwrap();
                let foreign = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
                assert!(matches!(
                    promote_cognition_history_view_v1(&selected, &accepted, &foreign, || false),
                    Err(CognitionHistorySelectionErrorV1::Value(
                        ValueTreeError::Invalid(_)
                    ))
                ));
                assert!(
                    promote_cognition_history_view_v1(&selected, &s.commit, &memory, || false)
                        .is_err()
                );
                let selected =
                    promote_cognition_history_view_v1(&selected, &accepted, &memory, || false)
                        .unwrap();
                assert_eq!(selected.occurrence().location().original, id);
                assert_eq!(selected.manifest().roots, s.roots.digest().unwrap());
                let advanced = normal::continue_view(
                    normal::Source {
                        memory: &memory,
                        config: &s.config,
                        roots: &s.roots,
                        authority: &s.authority,
                        reuse: None,
                    },
                    (&history.body, &history.prefix),
                    &catalog.after,
                    (&accepted, &selected),
                    &store,
                    &objects,
                    &mut io,
                );
                Some((
                    advanced.0,
                    advanced.1,
                    advanced.2,
                    s.config.clone(),
                    s.authority.clone(),
                ))
            }
        };
        if let Some((head, roots, id, config, authority)) = accepted {
            // Only fixed trusted head/config/authority inputs survive. All old engine pools,
            // objects, sources, core/current tokens and original bytes are gone.
            // 4MiB Ordinary cold/bridge negative remains mode 0. The complete
            // producer uses the pre-existing finite 32MiB envelope: catalog
            // workspace 2MiB + physical decoder 2MiB + transaction/metadata.
            let total_bytes = if mode >= 4 { 32 << 20 } else { 8 << 20 };
            let ordinary_limit = total_bytes - (4 << 20);
            let memory = MemoryBudget::new(total_bytes, 4 << 20).unwrap();
            let store = IncrementalPageStore::open(
                &path,
                memory.clone(),
                PageCompression::Raw,
                MemoryClass::Recovery,
            )
            .unwrap();
            let cancelled = AtomicBool::new(false);
            let mut physical = physical();
            let mut logical = logical();
            let objects = IncrementalObjectStore::open(
                &store,
                MemoryClass::Recovery,
                &mut physical,
                &cancelled,
            )
            .unwrap();
            let state = store
                .state_session(super::physical(), MemoryClass::Recovery)
                .unwrap();
            let mut io = ObjectIo {
                physical: &mut physical,
                logical: &mut logical,
                class: MemoryClass::Recovery,
                cancelled: &cancelled,
            };
            // Keep all Ordinary capacity occupied throughout actual cold header
            // and accepted-catalog reads. No fallback or larger pool.
            let _ordinary = memory
                .reserve(ordinary_limit - memory.used(), MemoryClass::Ordinary)
                .unwrap();
            assert!(memory.reserve(1, MemoryClass::Ordinary).is_err());
            let before = io.physical.reads_remaining;
            io.class = MemoryClass::Ordinary;
            assert!(matches!(
                objects.recovery_original_reader(&mut io),
                Err(PageStoreError::Corrupt(_))
            ));
            assert!(matches!(
                objects.original_reader(&mut io).read_original(
                    &id,
                    COGNITION_HISTORY_VIEW_BYTES,
                    &memory
                ),
                Err(PageStoreError::Memory(_))
            ));
            assert_eq!(io.physical.reads_remaining, before);
            io.class = MemoryClass::Recovery;
            assert!(matches!(
                objects.original_reader(&mut io).read_original(
                    &id,
                    COGNITION_HISTORY_VIEW_BYTES,
                    &memory
                ),
                Err(PageStoreError::Corrupt(_))
            ));
            let foreign = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
            {
                let mut reader = objects.recovery_original_reader(&mut io).unwrap();
                assert!(matches!(
                    reader.read_original(&id, COGNITION_HISTORY_VIEW_BYTES, &foreign),
                    Err(PageStoreError::Corrupt(_))
                ));
                let original = reader
                    .read_original(&id, COGNITION_HISTORY_VIEW_BYTES, &memory)
                    .unwrap();
                assert!(original.uses_budget(&memory));
                assert!(original.starts_with(COGNITION_HISTORY_VIEW_MAGIC));
            }
            // Pull parsing does not need an owned full original. Verification
            // and every range share this same Recovery-only IO session.
            {
                use std::io::Read;
                let mut cursor = objects
                    .verified_original_cursor(&id, COGNITION_HISTORY_VIEW_BYTES as u64, &mut io)
                    .unwrap();
                assert_eq!(cursor.length(), COGNITION_HISTORY_VIEW_BYTES as u64);
                let mut prefix = [0; 4];
                cursor.read_exact(&mut prefix).unwrap();
                assert!(matches!(cursor.finish(), Err(PageStoreError::Corrupt(_))));
                let mut cursor = objects
                    .verified_original_cursor(&id, COGNITION_HISTORY_VIEW_BYTES as u64, &mut io)
                    .unwrap();
                let mut bytes = [0; COGNITION_HISTORY_VIEW_BYTES];
                for part in bytes.chunks_mut(17) {
                    cursor.read_exact(part).unwrap();
                }
                assert_eq!(cursor.read(&mut prefix).unwrap(), 0);
                cursor.finish().unwrap();
                assert!(bytes.starts_with(COGNITION_HISTORY_VIEW_MAGIC));
                let mut cursor = objects
                    .verified_original_cursor(&id, COGNITION_HISTORY_VIEW_BYTES as u64, &mut io)
                    .unwrap();
                cursor.read_exact(&mut prefix).unwrap();
                cancelled.store(true, Ordering::Release);
                assert!(cursor.read(&mut prefix).is_err());
                assert!(matches!(
                    cursor.take_error(),
                    Some(PageStoreError::Cancelled)
                ));
                cancelled.store(false, Ordering::Release);
                assert!(cursor.read(&mut prefix).is_err());
                assert!(cursor.finish().is_err());
            }
            // A real root-bound absence audit, not SourceRef::Disabled or an
            // invalid enabled CurrentStateQualification with empty columns.
            let values = store.value_session(super::physical());
            let mut absence_io = super::logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let absence = if mode >= 4 {
                None
            } else {
                Some(oasis7_world_state::incremental_cognition_absence::qualify_cognition_current_absence_v1(&roots, &mut WorldFieldReadContextV1 { root: roots.state_root(StateNamespaceV1::Artifacts), state: &state, values: ValueReadContext { source: &values, memory: &memory, class: MemoryClass::Recovery, io: &mut absence_io }, key_work: &mut keys, semantic_bytes: 256, record_bytes: 4096, take_state_error: || state.take_read_error().map(state_error), cancelled: || false }, &mut oasis7_formal_protocol::incremental_state::StateScanBudget { nodes_remaining: 1024, read_bytes_remaining: 1 << 20, leaves_remaining: 128 }, &mut 128).unwrap())
            };
            let mut cold_core = core_budget();
            let mut cold_facts = self::facts();
            let location = oasis7_formal_protocol::incremental_material::FactLocationV1 {
                height: roots.height,
                ordinal: if mode >= 4 { 2 } else { 1 },
                original: id,
            };
            let raw = verify_fact_occurrence_v1(
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
                    core_budget: &mut cold_core,
                    fact_budget: &mut cold_facts,
                    cancelled: || false,
                },
            )
            .unwrap();
            assert_eq!(raw.kind(), ExecutionFactKindV1::Result);
            assert_eq!(raw.roots_id(), roots.digest().unwrap());
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
                    core_budget: &mut cold_core,
                    fact_budget: &mut cold_facts,
                    cancelled: || false,
                },
            )
            .unwrap();
            assert_eq!(
                selected.occurrence().head_body_id(),
                selected.occurrence().fact_body_id()
            );
            assert_eq!(selected.manifest().roots, roots.digest().unwrap());
            let mut recovery_index = None;
            if mode >= 4 {
                use oasis7_world_state::incremental_cognition_history_cbor::*;
                let index = selected
                    .manifest()
                    .index_ref(CognitionHistoryFieldV1::Responses);
                assert_eq!(index.records, 1);
                let mut row = None;
                recovery_index = Some(
                    qualify_cognition_history_index_v1(
                        CognitionHistoryFieldV1::Responses,
                        index,
                        &mut WorldFieldReadContextV1 {
                            root: &index.root,
                            state: &state,
                            values: ValueReadContext {
                                source: &values,
                                memory: &memory,
                                class: MemoryClass::Recovery,
                                io: &mut absence_io,
                            },
                            key_work: &mut keys,
                            semantic_bytes: 256,
                            record_bytes: 4096,
                            take_state_error: || state.take_read_error().map(state_error),
                            cancelled: || false,
                        },
                        &mut CognitionHistoryIndexWorkV1 {
                            cells_remaining: 32,
                            max_pending_node_bytes: 1 << 20,
                        },
                        |ordinal, original| {
                            assert_eq!(ordinal, 0);
                            assert!(row.replace(original).is_none());
                            Ok(())
                        },
                    )
                    .unwrap(),
                );
                let row = row.unwrap();
                assert_eq!((row.location.height, row.location.ordinal), (1, 1));
                let occurrence = verify_fact_occurrence_v1(
                    &FactRequestV1 {
                        kind: ExecutionFactKindV1::Result,
                        head: &head,
                        roots: &roots,
                        location: row.location,
                    },
                    &mut FactReadContextV1 {
                        originals: &mut objects.recovery_original_reader(&mut io).unwrap(),
                        catalog_pages: &state,
                        memory: &memory,
                        class: MemoryClass::Recovery,
                        core_budget: &mut cold_core,
                        fact_budget: &mut cold_facts,
                        cancelled: || false,
                    },
                )
                .unwrap();
                assert_eq!(occurrence.original_bytes(), row.encoded_bytes);
                let mut original = objects
                    .verified_original_cursor(&row.location.original, row.encoded_bytes, &mut io)
                    .unwrap();
                let record_schema = verify_cognition_history_original_record_schema_v1(
                    CognitionOriginalCborRequestV1 {
                        field: CognitionHistoryFieldV1::Responses,
                        id: row.location.original,
                        bytes: row.encoded_bytes,
                    },
                    &mut original,
                    &memory,
                    MemoryClass::Recovery,
                    &mut CognitionHistoryCborWorkV1 {
                        bytes_remaining: 64 << 10,
                        nodes_remaining: 10_000,
                        source_reads_remaining: 100_000,
                        key_bytes_remaining: 4096,
                        max_depth: 16,
                        max_key_bytes: 4096,
                    },
                    || false,
                )
                .unwrap();
                original.finish().unwrap();
                assert_eq!(
                    record_schema.scope(),
                    CognitionHistoryRecordScopeV1::ResponseWrapperFields
                );
                let semantic = record_schema.semantic();
                let expected = oasis7_world_state::cognition_recovery::response_artifact_digest(
                    &serde_json::json!({"reason":"actual original response"}),
                );
                assert_eq!(
                    semantic.identity_status(),
                    CognitionOriginalIdentityStatusV1::DigestMatched(
                        *blake3::Hash::from_hex(expected.strip_prefix("blake3:").unwrap())
                            .unwrap()
                            .as_bytes()
                    )
                );
                assert_eq!(
                    semantic.journal_head(),
                    Some(selected.manifest().heads[0].digest)
                );
                let mut point_read = |ordinal| {
                    read_cognition_history_index_point_v1(
                        recovery_index.as_ref().unwrap(),
                        ordinal,
                        &mut WorldFieldReadContextV1 {
                            root: &index.root,
                            state: &state,
                            values: ValueReadContext {
                                source: &values,
                                memory: &memory,
                                class: MemoryClass::Recovery,
                                io: &mut absence_io,
                            },
                            key_work: &mut keys,
                            semantic_bytes: 256,
                            record_bytes: 4096,
                            take_state_error: || state.take_read_error().map(state_error),
                            cancelled: || false,
                        },
                        &mut CognitionHistoryIndexWorkV1 {
                            cells_remaining: 1,
                            max_pending_node_bytes: 1 << 20,
                        },
                    )
                };
                assert!(matches!(point_read(1), Err(ValueTreeError::Invalid(_))));
                let point = point_read(0).unwrap();
                assert_eq!(point.row(), row);
                let joined = bind_indexed_cognition_original_v1(
                    point,
                    record_schema,
                    &occurrence,
                    &selected,
                    &memory,
                    || false,
                )
                .unwrap();
                assert_eq!(joined.ordinal(), 0);
                assert_eq!(joined.row(), row);
                assert_eq!(
                    joined.head_record_id(),
                    blake3::Hash::from_hex(&head.head().record_id)
                        .unwrap()
                        .as_bytes()
                );
                assert_eq!(joined.roots_id(), &roots.digest().unwrap());
                assert_eq!(
                    joined.schema().scope(),
                    CognitionHistoryRecordScopeV1::ResponseWrapperFields
                );
            }
            // The previous Result genuinely exists under the new accepted
            // prefix, but must not select current state.
            let previous = selected.manifest().previous_view.unwrap();
            assert!(matches!(
                recover_selected_cognition_history_view_v1(
                    &FactRequestV1 {
                        kind: ExecutionFactKindV1::Result,
                        head: &head,
                        roots: &roots,
                        location: previous
                    },
                    &mut FactReadContextV1 {
                        originals: &mut objects.recovery_original_reader(&mut io).unwrap(),
                        catalog_pages: &state,
                        memory: &memory,
                        class: MemoryClass::Recovery,
                        core_budget: &mut cold_core,
                        fact_budget: &mut cold_facts,
                        cancelled: || false,
                    },
                ),
                Err(CognitionHistorySelectionErrorV1::Value(
                    ValueTreeError::Invalid("Cognition selected view historical body")
                ))
            ));
            let bytes = objects
                .read_object_owned(&id, COGNITION_HISTORY_VIEW_BYTES, &mut io)
                .unwrap();
            let decoded = CognitionHistoryViewV1::from_canonical_bytes(
                &bytes,
                &memory,
                MemoryClass::Recovery,
                || false,
            )
            .unwrap();
            assert_eq!(decoded.roots, roots.digest().unwrap());
            assert_eq!(decoded.enabled, mode >= 4);
            assert_eq!(decoded.revision, 1);
            assert_eq!(decoded.previous_view.unwrap().height, 0);
            let current = if mode >= 4 {
                Some(enabled::qualify_current(
                    &roots,
                    &store,
                    &memory,
                    MemoryClass::Recovery,
                ))
            } else {
                None
            };
            let source = if let Some(current) = &current {
                CognitionHistoryCurrentSourceV1::Enabled(current)
            } else {
                CognitionHistoryCurrentSourceV1::Disabled(absence.as_ref().unwrap())
            };
            let binding = bind_cognition_history_current_v1(
                &selected,
                &roots,
                source,
                &mut WorldFieldReadContextV1 {
                    root: roots.state_root(StateNamespaceV1::Allocator),
                    state: &state,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class: MemoryClass::Recovery,
                        io: &mut absence_io,
                    },
                    key_work: &mut keys,
                    semantic_bytes: 4096,
                    record_bytes: 8192,
                    take_state_error: || state.take_read_error().map(state_error),
                    cancelled: || false,
                },
            )
            .unwrap();
            assert_eq!(binding.roots_id(), roots.digest().unwrap());
            assert_eq!(binding.enabled(), mode >= 4);
            let ready = readmission::run(readmission::Frame {
                selected: &selected,
                index: recovery_index.as_ref(),
                binding: &binding,
                current: if let Some(current) = &current {
                    CognitionHistoryCurrentSourceV1::Enabled(current)
                } else {
                    CognitionHistoryCurrentSourceV1::Disabled(absence.as_ref().unwrap())
                },
                head: &head,
                roots: &roots,
                memory: &memory,
                io: &mut io,
                source_path: &path,
                ordinary_hold: Some(_ordinary),
                ordinary_limit,
                expected_json_slots: if mode == 5 { Some(20) } else { None },
            });
            drop(selected);
            drop(recovery_index);
            let admitted = &ready.view;
            let ordinary_state = store
                .state_session(super::physical(), MemoryClass::Ordinary)
                .unwrap();
            let mut ordinary_io = super::logical();
            let mut ordinary_keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let source = if let Some(current) = &current {
                CognitionHistoryCurrentSourceV1::Enabled(current)
            } else {
                CognitionHistoryCurrentSourceV1::Disabled(absence.as_ref().unwrap())
            };
            assert!(matches!(
                bind_cognition_history_current_v1(
                    admitted,
                    &roots,
                    source,
                    &mut WorldFieldReadContextV1 {
                        root: roots.state_root(StateNamespaceV1::Allocator),
                        state: &ordinary_state,
                        values: ValueReadContext {
                            source: &values,
                            memory: &memory,
                            class: MemoryClass::Ordinary,
                            io: &mut ordinary_io
                        },
                        key_work: &mut ordinary_keys,
                        semantic_bytes: 256,
                        record_bytes: 4096,
                        take_state_error: || ordinary_state.take_read_error().map(state_error),
                        cancelled: || false,
                    },
                ),
                Err(ValueTreeError::Invalid(_))
            ));
            drop(current);
            drop(absence);
            drop(binding);
            let source = ready.current.current();
            let binding = bind_cognition_history_current_v1(
                admitted,
                &roots,
                source,
                &mut WorldFieldReadContextV1 {
                    root: roots.state_root(StateNamespaceV1::Allocator),
                    state: &ordinary_state,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut ordinary_io,
                    },
                    key_work: &mut ordinary_keys,
                    semantic_bytes: 4096,
                    record_bytes: 8192,
                    take_state_error: || ordinary_state.take_read_error().map(state_error),
                    cancelled: || false,
                },
            )
            .unwrap();
            assert_eq!(
                binding.head_record_id(),
                admitted.occurrence().head_record_id()
            );
            io.class = MemoryClass::Ordinary;
            let continuation = normal::continue_from_cold(
                normal::Source {
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
            );
            if mode == 0 {
                assert!(
                    matches!(continuation, Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Tree(ref message)) if message.starts_with("Memory(MemoryLimitExceeded"))
                );
                assert_eq!(ready.current.binding().roots_id(), roots.digest().unwrap());
                assert_eq!(
                    ready.current.binding().head_record_id(),
                    ready.view.occurrence().head_record_id()
                );
            } else {
                let (continued, after, _) = continuation.unwrap();
                assert_eq!(after.height, 2);
                assert_eq!(
                    continued.record().after_state_root,
                    hex::encode(after.digest().unwrap())
                );
            }
            let with_proofs = memory.used();
            drop(ready);
            assert!(memory.used() < with_proofs);
        }
    }
}
