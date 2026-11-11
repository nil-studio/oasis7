//! Actual signed active-scope changes. Previous accepted views remain retained;
//! these source proofs do not authorize business resets or terminal recovery.
use super::signed::{core_budget, facts, put};
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        self,
        test_fixtures::{self as fixture, incremental},
    },
    incremental_execution::{
        ExecutionFactKindV1, ExecutionFactRefV1, ExecutionParent, ExecutionTransaction, RootSetV1,
        TransactionLimits,
    },
    incremental_history::HeaderPreparation,
    incremental_material::{CoreRequest, verify_commit_core, verify_fact_originals},
    incremental_record_root::RecordRootRefV1,
    incremental_state::{StateNamespaceV1, StateRootV1, StateScanBudget, StateScanContext},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::{
    incremental_cognition_current_complete_v3::*,
    incremental_cognition_history_hash_outboard::*,
    incremental_cognition_lifecycle_qualification_v2::*,
    incremental_json_records::{JsonChangeWorkV1, JsonReadWorkV1},
};
fn hashes() -> CognitionHeadHashWorkV1 {
    CognitionHeadHashWorkV1 {
        input_bytes_remaining: 32 << 20,
        hash_nodes_remaining: 1_000_000,
    }
}
fn index_work() -> CognitionHistoryIndexWorkV1 {
    CognitionHistoryIndexWorkV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 8 << 20,
    }
}
fn current_work() -> CognitionCurrentCompleteWorkV3 {
    CognitionCurrentCompleteWorkV3 {
        scan: scan(),
        json: JsonChangeWorkV1 {
            json: JsonReadWorkV1 {
                cells_remaining: 100_000,
                json_nodes_remaining: 100_000,
                max_depth: 64,
                max_payload_bytes: 8 << 20,
            },
            paths_remaining: 100_000,
            max_pending_pairs: 1024,
        },
        outer_cells_remaining: 100_000,
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 128 << 20,
        leaves_remaining: 100_000,
    }
}
fn absent() -> CognitionHistoryIndexRefV1 {
    CognitionHistoryIndexRefV1 {
        present: false,
        records: 0,
        root: StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap(),
    }
}
fn persist_nodes<'a>(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    nodes: impl IntoIterator<Item = &'a [u8]>,
    io: &mut ObjectIo<'_>,
) {
    use oasis7_formal_protocol::incremental_value::ValueNodeWriter;
    let mut no_cancel = || false;
    let mut pages = store
        .buffered_page_writer(io.physical, io.cancelled, &mut no_cancel)
        .unwrap();
    for bytes in nodes {
        pages
            .put_node(
                *blake3::hash(bytes).as_bytes(),
                memory
                    .reserve(bytes.len() + 128, MemoryClass::Ordinary)
                    .unwrap()
                    .retain(bytes.to_vec()),
            )
            .unwrap();
    }
    pages.finish().unwrap();
}
fn checked_scope_oracle(
    request: &CognitionHistorySelectionRequestV1<'_>,
    parent: Option<&ReservedValue<CompleteCognitionHistorySourceV3>>,
    store: &IncrementalPageStore,
    objects: &IncrementalObjectStore<'_>,
    io: &mut ObjectIo<'_>,
    memory: &MemoryBudget,
) -> ReservedValue<CheckedCognitionHistorySourceV3> {
    let selected = check_candidate_cognition_history_view_v1(
        request,
        &mut objects.original_reader(io),
        memory,
        &mut facts(),
        || false,
    )
    .unwrap();
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut quota = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut read = WorldFieldReadContextV1 {
        root: request.core.roots().state_root(StateNamespaceV1::Allocator),
        state: &state,
        values: ValueReadContext {
            source: &values,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut quota,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 8192,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    let presence = if selected.manifest().enabled {
        CognitionCurrentPresenceV3::Enabled
    } else {
        CognitionCurrentPresenceV3::Disabled
    };
    let current = qualify_complete_cognition_current_v3(
        request.core.roots(),
        presence,
        &mut read,
        &mut current_work(),
    )
    .unwrap();
    let bound = bind_candidate_cognition_history_current_v3(
        request,
        &selected,
        request.core.roots(),
        current.history_source(),
        &mut read,
    )
    .unwrap();
    if !selected.manifest().enabled {
        let CognitionHistoryCurrentSourceV1::Disabled(absence) = current.history_source() else {
            panic!("actual absent current")
        };
        let disabled = match parent {
            None => check_candidate_disabled_cognition_history_source_v3(
                request,
                &selected,
                absence,
                None,
                memory,
                || false,
            ),
            Some(parent) => check_candidate_disabled_cognition_history_successor_v3(
                request,
                &selected,
                absence,
                parent,
                memory,
                || false,
            ),
        }
        .unwrap();
        return qualify_checked_cognition_history_source_v3(
            CheckedCognitionHistoryComponentsV3 {
                selected,
                current: bound,
                indexes: std::array::from_fn(|_| None),
                heads: std::array::from_fn(|_| None),
                lifecycle: None,
                disabled: Some(disabled),
            },
            memory,
            || false,
        )
        .unwrap();
    }
    let mut indexes = std::array::from_fn(|_| None);
    let mut full = std::array::from_fn::<_, 2, _>(|_| None);
    for slot in 0..2 {
        let field = CognitionHistoryFieldV1::ALL[slot];
        let q = qualify_cognition_history_index_v1(
            field,
            selected.manifest().index_ref(field),
            &mut read,
            &mut index_work(),
            |_, _| panic!("empty active scope test"),
        )
        .unwrap();
        let source = match parent {
            None => CandidateCognitionHistoryIndexBuilderV3::new(
                request,
                &selected,
                field,
                Some(&q),
                None,
                memory,
                || false,
            )
            .unwrap(),
            Some(parent) if parent.selected().manifest().index_ref(field).present => {
                CandidateCognitionHistoryIndexBuilderV3::new(
                    request,
                    &selected,
                    field,
                    None,
                    Some((parent.indexes()[slot].as_ref().unwrap(), parent.selected())),
                    memory,
                    || false,
                )
                .unwrap()
            }
            Some(parent) => CandidateCognitionHistoryIndexBuilderV3::activate_field(
                request,
                &selected,
                parent,
                &q,
                memory,
                || false,
            )
            .unwrap(),
        }
        .finish(|| false)
        .unwrap();
        indexes[slot] = Some(source);
        full[slot] = Some(q);
    }
    let cache = recover_cognition_head_cache_v1(
        selected.derived().unwrap().caches[0].unwrap(),
        &mut read.values,
        &mut hashes(),
        || false,
    )
    .unwrap();
    let journal = full[0].as_ref().unwrap();
    let head = match parent {
        None => CandidateCognitionHeadSourceBuilderV3::genesis(
            request,
            &selected,
            journal,
            memory,
            || false,
        )
        .unwrap(),
        Some(parent) if parent.selected().manifest().enabled => {
            assert!(
                CandidateCognitionHeadSourceBuilderV3::activate_field(
                    request,
                    &selected,
                    parent,
                    journal,
                    memory,
                    || false
                )
                .is_err(),
                "an existing active field cannot be reinitialized"
            );
            CandidateCognitionHeadSourceBuilderV3::ordinary(
                request,
                &selected,
                parent.selected(),
                parent.heads()[0].as_ref().unwrap(),
                parent.indexes()[0].as_ref().unwrap(),
                memory,
                || false,
            )
            .unwrap()
        }
        Some(parent) => {
            assert!(
                CandidateCognitionHeadSourceBuilderV3::genesis(
                    request,
                    &selected,
                    journal,
                    memory,
                    || false
                )
                .is_err(),
                "non-GEN activation cannot claim bootstrap"
            );
            CandidateCognitionHeadSourceBuilderV3::activate_field(
                request,
                &selected,
                parent,
                journal,
                memory,
                || false,
            )
            .unwrap()
        }
    }
    .finish(&cache, &mut hashes(), || false)
    .unwrap();
    let lifecycle = match parent {
        None => {
            CandidateCognitionLifecycleBuilderV3::new(request, &selected, None, memory, || false)
                .unwrap()
        }
        Some(parent) if parent.selected().manifest().enabled => {
            CandidateCognitionLifecycleBuilderV3::new(
                request,
                &selected,
                Some((parent.lifecycle_parent().unwrap(), parent.selected())),
                memory,
                || false,
            )
            .unwrap()
        }
        Some(parent) => CandidateCognitionLifecycleBuilderV3::activate(
            request,
            &selected,
            parent,
            memory,
            || false,
        )
        .unwrap(),
    }
    .finish(|| false)
    .unwrap();
    qualify_checked_cognition_history_source_v3(
        CheckedCognitionHistoryComponentsV3 {
            selected,
            current: bound,
            indexes,
            heads: [Some(head), None, None],
            lifecycle: Some(lifecycle),
            disabled: None,
        },
        memory,
        || false,
    )
    .unwrap()
}
fn checked_scope(
    request: &CognitionHistorySelectionRequestV1<'_>,
    parent: Option<&ReservedValue<CompleteCognitionHistorySourceV3>>,
    store: &IncrementalPageStore,
    objects: &IncrementalObjectStore<'_>,
    io: &mut ObjectIo<'_>,
    memory: &MemoryBudget,
) -> ReservedValue<CheckedCognitionHistorySourceV3> {
    let oracle = checked_scope_oracle(request, parent, store, objects, io, memory);
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut quota = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut read = WorldFieldReadContextV1 {
        root: request.core.roots().state_root(StateNamespaceV1::Allocator),
        state: &state,
        values: ValueReadContext {
            source: &values,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut quota,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 8192,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    let current = qualify_complete_cognition_current_v3(
        request.core.roots(),
        if oracle.selected().manifest().enabled {
            CognitionCurrentPresenceV3::Enabled
        } else {
            CognitionCurrentPresenceV3::Disabled
        },
        &mut read,
        &mut current_work(),
    )
    .unwrap();
    let production = super::source_reader::check(
        request,
        current.history_source(),
        parent,
        store,
        objects,
        io,
        memory,
    );
    assert_eq!(
        production.candidate_record_id(),
        oracle.candidate_record_id()
    );
    assert_eq!(production.body_id(), oracle.body_id());
    assert_eq!(
        production.selected().manifest(),
        oracle.selected().manifest()
    );
    production
}
#[test]
fn source_presence_four_actual_signed_transitions_preserve_previous_scope() {
    for (before_enabled, after_enabled) in
        [(false, false), (false, true), (true, false), (true, true)]
    {
        let directory = Directory::new();
        let memory = MemoryBudget::new(256 << 20, 16 << 20).unwrap();
        let _fixture = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
        let store = IncrementalPageStore::create(
            &directory.0.join("source-presence"),
            memory.clone(),
            PageCompression::Raw,
        )
        .unwrap();
        let enabled = enabled::genesis(&store, &memory, false);
        let config = incremental::genesis_config([4; 32]);
        let scope = config.scope().unwrap();
        let mut roots = RootSetV1::empty(scope.world_id.clone(), scope.genesis_config).unwrap();
        if before_enabled {
            roots.state = enabled.state.clone();
        }
        let stopped = AtomicBool::new(false);
        let mut physical_io = physical();
        let mut logical_io = logical();
        let objects = IncrementalObjectStore::create(&store, &mut physical_io, &stopped).unwrap();
        let mut io = ObjectIo {
            physical: &mut physical_io,
            logical: &mut logical_io,
            class: MemoryClass::Ordinary,
            cancelled: &stopped,
        };
        let cache = CognitionHeadHashBuilderV1::new(&memory, MemoryClass::Ordinary, || false)
            .unwrap()
            .finish(|| false)
            .unwrap();
        let cache_id = persist_cognition_head_cache_v1(
            &cache,
            &mut store.value_session(physical()),
            &memory,
            &mut logical(),
            &mut hashes(),
            || false,
        )
        .unwrap();
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
        let mut cache_digest = Digest(blake3::Hasher::new());
        cache
            .write_canonical_cache_v1(&mut cache_digest, &mut hashes(), || false)
            .unwrap();
        let cache_digest = *cache_digest.0.finalize().as_bytes();
        let make = |enabled_now: bool, roots_id: [u8; 32], revision: u64, previous_view| {
            let view = CognitionHistoryViewV1 {
                roots: roots_id,
                enabled: enabled_now,
                revision,
                previous_view,
                heads: std::array::from_fn(|slot| CognitionHistoryHeadV1 {
                    sequence: 0,
                    digest: if enabled_now {
                        enabled.heads[slot]
                    } else {
                        None
                    },
                }),
                indexes: if enabled_now {
                    enabled.indexes.clone()
                } else {
                    std::array::from_fn(|_| absent())
                },
            };
            let refs = CognitionHistoryDerivedRefsV2 {
                caches: [enabled_now.then_some(cache_id), None, None],
                lifecycle: enabled_now.then(|| {
                    RecordRootRefV1::new(
                        lifecycle_schema(),
                        lifecycle_owner(),
                        StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap(),
                    )
                    .unwrap()
                }),
            };
            let summary = CognitionHistorySourceSummaryV3 {
                mode: if revision == 0 {
                    CognitionHistorySourceModeV3::Bootstrap
                } else {
                    CognitionHistorySourceModeV3::Delta
                },
                cache_state_digests: [enabled_now.then_some(cache_digest), None, None],
                last: std::array::from_fn(|_| None),
                lifecycle_keys: 0,
                lifecycle_journal_records: 0,
            };
            (view, refs, summary)
        };
        let (view, refs, summary) = make(before_enabled, roots.digest().unwrap(), 0, None);
        let result = super::source_v3::persist_v3_summary(
            &view,
            &refs,
            &summary,
            &memory,
            &objects,
            &mut io,
            || false,
        )
        .unwrap();
        let (certificate, _, _, _) = fixture::initial();
        let decode = |value: &str| hex::decode(value).unwrap().try_into().unwrap();
        let setup = incremental::setup_with_config_state_and_facts(
            memory.clone(),
            config,
            roots.state.clone(),
            incremental::FixtureExecutionArtifactRefsV1 {
                manifest: decode(&certificate.body.runtime_manifest_digest),
                wasm: [47; 32],
                rules: [48; 32],
                policy: decode(&certificate.body.execution_policy_digest),
            },
            &[result],
        );
        let (blobs, catalog) = incremental::core_objects(&setup, &setup.history);
        for blob in blobs.iter() {
            put(&objects, blob.as_slice(), &mut io);
        }
        persist_nodes(
            &store,
            &memory,
            catalog.new_nodes.iter().map(|node| node.bytes.as_slice()),
            &mut io,
        );
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let candidate = formal::verify_formal_candidate(
            &formal::SignedCanonicalCandidateV1 {
                record: setup.proof.decision.record.clone(),
                writer_signature_hex: setup.proof.decision.writer_signature_hex.clone(),
            },
            None,
            &setup.authority,
            None,
            None,
            formal::FormalCompletionBranchV1::Normal,
        )
        .unwrap();
        let core = verify_commit_core(
            &CoreRequest {
                candidate: &candidate,
                original_authority: &setup.authority,
                parent: None,
                max_pending_node_bytes: 8 << 20,
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
        let checked = checked_scope(
            &CognitionHistorySelectionRequestV1 {
                candidate: &candidate,
                core: &core,
                originals: &originals,
            },
            None,
            &store,
            &objects,
            &mut io,
            &memory,
        );
        let pending = super::source_reader::prepare(&checked, &memory);
        let actual_genesis =
            formal::verify_formal_qualified(&setup.proof, &setup.authority, None, None).unwrap();
        let parent =
            super::source_reader::finish_without_admission(pending, &actual_genesis, &memory);
        let execution_parent = ExecutionParent {
            roots: setup.roots.clone(),
            commit: setup.history.body.digest().unwrap(),
            authority: setup.history.body.authority,
        };
        let live = execution_parent.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(
            execution_parent,
            [91; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 256,
                max_writes: 256,
                max_facts: 4,
                max_pending_node_bytes: 16 << 20,
            },
        )
        .unwrap();
        let input = *blake3::hash(b"actual presence scope input").as_bytes();
        put(&objects, b"actual presence scope input", &mut io);
        tx.append_fact(ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Input,
            id: input,
        })
        .unwrap();
        if before_enabled != after_enabled {
            let mut budget = scan();
            for root in &enabled.state {
                StateScanContext {
                    source: &state,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut budget,
                }
                .visit_entries(
                    root,
                    |entry| {
                        tx.write(
                            entry.namespace,
                            &entry.key,
                            after_enabled.then_some(entry.value),
                        )
                    },
                    || false,
                )
                .unwrap();
            }
        }
        let guard = tx.guard();
        let mut after_manifest = None;
        let sealed = tx
            .seal_with_final_fact(
                &guard,
                &live,
                1,
                || false,
                |after| {
                    let (manifest, refs, summary) = make(
                        after_enabled,
                        after.digest().unwrap(),
                        1,
                        Some(parent.selected().occurrence().location()),
                    );
                    let fact = super::source_v3::persist_v3_summary(
                        &manifest,
                        &refs,
                        &summary,
                        &memory,
                        &objects,
                        &mut io,
                        || false,
                    )?;
                    after_manifest = Some(manifest);
                    Ok::<_, ValueTreeError>(fact)
                },
            )
            .unwrap();
        persist_nodes(
            &store,
            &memory,
            sealed.new_nodes.iter().map(|node| node.bytes.as_slice()),
            &mut io,
        );
        let catalog = setup
            .history
            .body
            .catalog_before
            .extend(&setup.history.body, &state, &memory, 8 << 20)
            .unwrap();
        let history = HeaderPreparation {
            config: &setup.config,
            catalog: &catalog.after,
            roots: &sealed.record.after,
            parent: Some(&setup.history.body),
            parent_history: Some(&setup.history.prefix),
            transition: Some(&sealed.record),
            genesis_facts: &[],
            operation: [91; 32],
            authority: setup.history.body.authority,
            execution_environment: setup.config.execution_environment,
            checkpoint: false,
        }
        .prepare(&memory)
        .unwrap();
        let transition_pages = u64::try_from(sealed.new_nodes.len()).unwrap();
        let catalog_pages = u64::try_from(
            history
                .body
                .catalog_before
                .extend(&history.body, &state, &memory, 4 << 20)
                .unwrap()
                .new_nodes
                .len(),
        )
        .unwrap();
        let mut core_work = core_budget();
        let default_new_pages = core_work.new_pages_remaining;
        core_work.new_pages_remaining = transition_pages.checked_add(catalog_pages).unwrap();
        eprintln!(
            "source presence {before_enabled}->{after_enabled}: transition_pages={transition_pages}, catalog_pages={catalog_pages}, required_new_pages={}, old_partial_allowance={default_new_pages}",
            core_work.new_pages_remaining
        );
        let (accepted, selected, _extension, checked) =
            super::normal_accept::accept_normal_history_with_candidate_work(
                super::normal_accept::NormalHistoryFrame {
                    authority: &setup.authority,
                    parent: &setup.commit,
                    after: &sealed.record.after,
                    history: &history,
                    input,
                    expected_facts: 2,
                },
                &memory,
                &store,
                &objects,
                &mut io,
                &mut core_work,
                |candidate, core, originals, _selected, io| {
                    let checked = checked_scope(
                        &CognitionHistorySelectionRequestV1 {
                            candidate,
                            core,
                            originals,
                        },
                        Some(&parent),
                        &store,
                        &objects,
                        io,
                        &memory,
                    );
                    (
                        super::source_reader::prepare(&checked, &memory),
                        super::source_reader::prepare(&checked, &memory),
                    )
                },
            );
        assert_eq!(
            core_work.new_pages_remaining, 0,
            "exact current transition plus catalog page charge"
        );
        let (pending, wrong_head_pending) = checked;
        assert!(
            finish_prepared_complete_cognition_history_promotion_v3(
                wrong_head_pending,
                &setup.commit
            )
            .is_err(),
            "a true accepted GEN is not the checked Normal head"
        );
        let after = super::source_reader::finish_without_admission(pending, &accepted, &memory);
        assert_eq!(
            after.selected().manifest(),
            after_manifest.as_ref().unwrap()
        );
        assert_eq!(
            selected.manifest().previous_view,
            Some(parent.selected().occurrence().location())
        );
        assert_eq!(after.current().enabled(), after_enabled);
        assert_eq!(after.indexes()[0].is_some(), after_enabled);
        assert_eq!(after.heads()[0].is_some(), after_enabled);
        assert_eq!(after.lifecycle_parent().is_some(), after_enabled);
        assert_eq!(after.disabled().is_some(), !after_enabled);
        // The previous accepted selected bytes and originals are never edited.
        let old = objects
            .read_object_owned(
                &parent.selected().occurrence().location().original,
                1 << 20,
                &mut io,
            )
            .unwrap();
        assert_eq!(
            blake3::hash(&old).as_bytes(),
            &parent.selected().occurrence().location().original
        );
    }
}
