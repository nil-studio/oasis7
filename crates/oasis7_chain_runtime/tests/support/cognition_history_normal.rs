use super::signed::{core_budget, facts, put};
use super::*;
use oasis7_chain_runtime::incremental_cognition_history_store::persist_cognition_history_view_result_v1;
use oasis7_formal_protocol::{
    controlled_authority::formal,
    incremental_execution::{ExecutionFactKindV1, ExecutionFactRefV1, RootSetV1},
    incremental_history::{
        CatalogRootV1, CommitBodyV1, GenesisConfigV1, HeaderPreparation, HistoryPrefixV1,
    },
    incremental_state::StateNamespaceV1,
};
pub(super) struct Source<'a> {
    pub memory: &'a MemoryBudget,
    pub config: &'a GenesisConfigV1,
    pub roots: &'a RootSetV1,
    pub authority: &'a formal::VerifiedAuthorityV1,
    pub alternate_parent: Option<&'a formal::VerifiedFormalCommitV1>,
    pub reuse: Option<&'a readmission::OrdinaryFixtureProofs>,
}
pub(super) fn continue_from_cold(
    source: Source<'_>,
    parent: &formal::VerifiedFormalCommitV1,
    store: &IncrementalPageStore,
    objects: &IncrementalObjectStore<'_>,
    io: &mut ObjectIo<'_>,
) -> Result<
    (formal::VerifiedFormalCommitV1, RootSetV1, [u8; 32]),
    oasis7_formal_protocol::incremental_execution::ExecutionError,
> {
    let ready = source.reuse.unwrap();
    let _headers = source
        .memory
        .reserve(32 << 10, MemoryClass::Ordinary)
        .unwrap();
    let body_bytes = objects
        .read_object_owned(&ready.view.occurrence().head_body_id(), 4096, io)
        .unwrap();
    let body = CommitBodyV1::from_canonical_bytes(&body_bytes).unwrap();
    let prefix_bytes = objects.read_object_owned(&body.history, 4096, io).unwrap();
    let prefix = HistoryPrefixV1::from_canonical_bytes(&prefix_bytes).unwrap();
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let catalog = body
        .catalog_before
        .extend(&body, &state, source.memory, 4 << 20)?;
    Ok(continue_view(
        source,
        (&body, &prefix),
        &catalog.after,
        (parent, &ready.view),
        store,
        objects,
        io,
    ))
}

// Both legal disabled and enabled response cohorts require a genuine normal transaction,
// after-root final Result, parent-prefix extension and current signatures.
pub(super) fn continue_view(
    s: Source<'_>,
    parent_history: (&CommitBodyV1, &HistoryPrefixV1),
    catalog: &CatalogRootV1,
    parent: (
        &formal::VerifiedFormalCommitV1,
        &oasis7_proto::memory_budget::ReservedValue<AcceptedCognitionHistoryViewV1>,
    ),
    store: &IncrementalPageStore,
    objects: &IncrementalObjectStore<'_>,
    io: &mut ObjectIo<'_>,
) -> (
    formal::VerifiedFormalCommitV1,
    oasis7_formal_protocol::incremental_execution::RootSetV1,
    [u8; 32],
) {
    use oasis7_formal_protocol::incremental_execution::{
        ExecutionParent, ExecutionTransaction, TransactionLimits,
    };
    let memory = s.memory;
    let _scratch = memory.reserve(512 << 10, MemoryClass::Ordinary).unwrap();
    assert_eq!(
        hex::encode(parent.1.occurrence().head_record_id()),
        parent.0.record().record_id().unwrap()
    );
    assert_eq!(parent.1.manifest().roots, s.roots.digest().unwrap());
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let enabled = parent.1.manifest().enabled;
    let current = (enabled && s.reuse.is_none())
        .then(|| enabled::qualify_current(s.roots, store, memory, MemoryClass::Ordinary));
    let values = store.value_session(physical());
    let mut read_io = logical();
    let mut read_keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut read = WorldFieldReadContextV1 {
        root: s.roots.state_root(StateNamespaceV1::Allocator),
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
    let fresh_binding = current.as_ref().map(|proof| {
        bind_cognition_history_current_v1(
            parent.1,
            s.roots,
            CognitionHistoryCurrentSourceV1::Enabled(proof),
            &mut read,
        )
        .unwrap()
    });
    let fresh_index = current.as_ref().map(|_| {
        let index = parent
            .1
            .manifest()
            .index_ref(CognitionHistoryFieldV1::Responses);
        read.root = &index.root;
        qualify_cognition_history_index_v1(
            CognitionHistoryFieldV1::Responses,
            index,
            &mut read,
            &mut CognitionHistoryIndexWorkV1 {
                cells_remaining: 8,
                max_pending_node_bytes: 1 << 20,
            },
            |_, _| panic!("fresh empty response index"),
        )
        .unwrap()
    });
    let current_binding = s
        .reuse
        .map(|proofs| proofs.current.binding())
        .or(fresh_binding.as_ref());
    let index = s
        .reuse
        .and_then(|proofs| proofs.index.as_ref())
        .or(fresh_index.as_ref());
    if enabled {
        read.root = &parent
            .1
            .manifest()
            .index_ref(CognitionHistoryFieldV1::Responses)
            .root;
    }
    let lifecycle_parent = if parent.1.derived().is_some() {
        use oasis7_world_state::incremental_cognition_lifecycle_qualification_v2::*;
        let journal = parent
            .1
            .manifest()
            .index_ref(CognitionHistoryFieldV1::JournalEvents);
        read.root = &journal.root;
        let qualified = qualify_cognition_history_index_v1(
            CognitionHistoryFieldV1::JournalEvents,
            journal,
            &mut read,
            &mut CognitionHistoryIndexWorkV1 {
                cells_remaining: 8,
                max_pending_node_bytes: 1 << 20,
            },
            |_, _| panic!("empty parent journal"),
        )
        .unwrap();
        let lifecycle = qualify_empty_cognition_lifecycle_v2(
            parent.1,
            current_binding.unwrap(),
            &qualified,
            memory,
            || false,
        )
        .unwrap();
        read.root = &parent
            .1
            .manifest()
            .index_ref(CognitionHistoryFieldV1::Responses)
            .root;
        Some((lifecycle, qualified))
    } else {
        None
    };
    if enabled {
        use oasis7_world_state::incremental_cognition_history_hash_outboard::*;
        let caches = std::array::from_fn(|_| {
            CognitionHeadHashBuilderV1::new(memory, MemoryClass::Ordinary, || false)
                .unwrap()
                .finish(|| false)
                .unwrap()
        });
        let bound = bind_cognition_head_caches_v1(
            caches,
            parent.1,
            current_binding.unwrap(),
            memory,
            &mut CognitionHeadHashWorkV1 {
                input_bytes_remaining: 0,
                hash_nodes_remaining: 1024,
            },
            || false,
        )
        .unwrap();
        assert_eq!(bound.roots_id(), s.roots.digest().unwrap());
        assert_eq!(
            bound.head_record_id(),
            parent.1.occurrence().head_record_id()
        );
        for kind in CognitionHeadHashV1::ALL {
            assert_eq!(bound.state(kind).body_bytes(), 0);
        }
        let mut invalid =
            CognitionHeadHashBuilderV1::new(memory, MemoryClass::Ordinary, || false).unwrap();
        invalid
            .append(
                b"x",
                &mut CognitionHeadHashWorkV1 {
                    input_bytes_remaining: 1,
                    hash_nodes_remaining: 1024,
                },
                || false,
            )
            .unwrap();
        let bad = [
            invalid.finish(|| false).unwrap(),
            CognitionHeadHashBuilderV1::new(memory, MemoryClass::Ordinary, || false)
                .unwrap()
                .finish(|| false)
                .unwrap(),
            CognitionHeadHashBuilderV1::new(memory, MemoryClass::Ordinary, || false)
                .unwrap()
                .finish(|| false)
                .unwrap(),
        ];
        assert!(
            bind_cognition_head_caches_v1(
                bad,
                parent.1,
                current_binding.unwrap(),
                memory,
                &mut CognitionHeadHashWorkV1 {
                    input_bytes_remaining: 0,
                    hash_nodes_remaining: 1024
                },
                || false
            )
            .is_err()
        );
    }
    let execution_parent = ExecutionParent {
        roots: (*s.roots).clone(),
        commit: parent_history.0.digest().unwrap(),
        authority: parent_history.0.authority,
    };
    let fence = execution_parent.fence().unwrap();
    let mut tx = ExecutionTransaction::begin(
        execution_parent,
        [60; 32],
        &state,
        memory,
        TransactionLimits {
            max_reads: 4,
            max_writes: 4,
            max_facts: if enabled { 3 } else { 2 },
            max_pending_node_bytes: 4 << 20,
        },
    )
    .unwrap();
    let input_bytes = b"Cognition disabled normal input";
    let input = *blake3::hash(input_bytes).as_bytes();
    put(objects, input_bytes, io);
    tx.append_fact(ExecutionFactRefV1 {
        kind: ExecutionFactKindV1::Input,
        id: input,
    })
    .unwrap();
    assert_eq!(tx.ordered_fact_count(), 1);
    let guard = tx.guard();
    let response_after = index.map(|index| {
        let artifact = serde_json::json!({"reason":"actual original response"});
        let digest = oasis7_world_state::cognition_recovery::response_artifact_digest(&artifact);
        let response = serde_json::json!({"response_digest":digest,"response_artifact":artifact,"envelope_digest":"原 envelope", "journal_head":format!("blake3:{}",blake3::Hash::from(parent.1.manifest().heads[0].digest.unwrap()).to_hex())});
        let mut original = CognitionHistoryWriteContextV1::new(memory,
            CanonicalCborWorkV1 { visits_remaining: 10_000, bytes_remaining: 1 << 20, max_depth: 16, max_map_keys: 64, max_key_bytes: 4096 },
            1, || false,
        );
        let mut writer = store.value_session(physical());
        let mut write_io = logical();
        let mut write_keys = SemanticKeyBudget { bytes_remaining: 1 << 20 };
        contribute_cognition_response_append_v1(
            CognitionResponseAppendRequestV1 {
                record: CognitionHistoryRecordRefV1::from_original(CognitionHistoryFieldV1::Responses, index.record_count(), &response).unwrap(),
                parent_view: parent.1, parent_current: current_binding.unwrap(), parent_index: index, guard: &guard, live: &fence,
            }, &mut tx, &mut read,
            &mut MemberRecordWriteContext {
                sink: &mut writer, memory, io: &mut write_io, key_work: &mut write_keys,
                limits: MemberRecordLimitsV1 { semantic_bytes: 256, payload_bytes: 256, record_bytes: 4096 }, cancelled: || false,
            }, &mut original,
            &mut CognitionHistoryObjectSinkV1 { objects, io, cancelled: || false },
            &mut CognitionHistoryIndexWorkV1 { cells_remaining: 8, max_pending_node_bytes: 1 << 20 },
        ).unwrap()
    });
    let mut manifest_id = None;
    // Appending the response fact advanced the precise transaction revision.
    let guard = tx.guard();
    let sealed = tx
        .seal_with_final_fact(
            &guard,
            &fence,
            s.roots.simulation_tick.checked_add(1).unwrap(),
            || false,
            |after| {
                let mut view = parent.1.manifest().clone();
                view.roots = after.digest().unwrap();
                view.revision = parent.1.manifest().revision.checked_add(1).unwrap();
                view.previous_view = Some(parent.1.occurrence().location());
                if let Some(index) = &response_after {
                    view.indexes[1].records = index.record_count();
                    view.indexes[1].root = index.root().clone();
                }
                let fact = if let Some(derived) = parent.1.derived() {
                    oasis7_chain_runtime::incremental_cognition_history_store::persist_cognition_history_view_result_v2(after,&view,derived,memory,objects,io,||false)?
                } else {persist_cognition_history_view_result_v1(after,&view,memory,objects,io,||false)?};
                manifest_id = Some(fact.id);
                Ok::<_, ValueTreeError>(fact)
            },
        )
        .unwrap();
    let verification_parent = ExecutionParent {
        roots: (*s.roots).clone(),
        commit: fence.parent_commit,
        authority: fence.authority,
    };
    let after_current = if enabled {
        let before = if let Some(reused) = s.reuse {
            match reused.current.current() {
                CognitionHistoryCurrentSourceV1::Enabled(proof) => proof,
                _ => panic!("enabled Ordinary source"),
            }
        } else {
            current.as_ref().unwrap()
        };
        read.root = s.roots.state_root(StateNamespaceV1::Allocator);
        Some(oasis7_world_state::incremental_cognition_family::accept_cognition_current_transition_v1(
            before,
            &oasis7_formal_protocol::incremental_execution::TransitionVerifier { parent: &verification_parent, operation: [60;32], source: &state, memory, class: MemoryClass::Ordinary, max_pending_node_bytes: 4 << 20 },
            &sealed.record, &mut read,
            &mut oasis7_world_state::incremental_json_records::JsonScalarChangeWorkV1 {
                paths_remaining: 0, max_pending_pairs: 0,
                json: oasis7_world_state::incremental_json_records::JsonReadWorkV1 { cells_remaining: 0, json_nodes_remaining: 0, max_depth: 8, max_payload_bytes: 64 << 20 },
            },
        ).unwrap())
    } else {
        None
    };
    let history = HeaderPreparation {
        config: s.config,
        catalog,
        roots: &sealed.record.after,
        parent: Some(parent_history.0),
        parent_history: Some(parent_history.1),
        transition: Some(&sealed.record),
        genesis_facts: &[],
        operation: [60; 32],
        authority: parent_history.0.authority,
        execution_environment: s.config.execution_environment,
        checkpoint: false,
    }
    .prepare(memory)
    .unwrap();
    let (accepted, selected, extension) = super::normal_accept::accept_normal_history(
        super::normal_accept::NormalHistoryFrame {
            authority: s.authority,
            parent: s.alternate_parent.unwrap_or(parent.0),
            after: &sealed.record.after,
            history: &history,
            input,
            expected_facts: if enabled { 3 } else { 2 },
        },
        memory,
        store,
        objects,
        io,
    );
    assert_eq!(
        selected.manifest().previous_view,
        Some(parent.1.occurrence().location())
    );
    assert_eq!(selected.occurrence().location().height, s.roots.height + 1);
    assert_eq!(
        selected.occurrence().location().original,
        manifest_id.unwrap()
    );
    if let Some(after_index) = &response_after {
        use oasis7_formal_protocol::incremental_material::{
            FactReadContextV1, FactRequestV1, verify_fact_occurrence_v1,
        };
        use oasis7_world_state::incremental_cognition_history_cbor::*;
        read.root = after_index.root();
        let point = read_cognition_history_index_point_v1(
            after_index,
            index.unwrap().record_count(),
            &mut read,
            &mut CognitionHistoryIndexWorkV1 {
                cells_remaining: 1,
                max_pending_node_bytes: 1 << 20,
            },
        )
        .unwrap();
        let row = point.row();
        let occurrence = verify_fact_occurrence_v1(
            &FactRequestV1 {
                kind: ExecutionFactKindV1::Result,
                head: &accepted,
                roots: &sealed.record.after,
                location: row.location,
            },
            &mut FactReadContextV1 {
                originals: &mut objects.original_reader(io),
                catalog_pages: &state,
                memory,
                class: MemoryClass::Ordinary,
                core_budget: &mut core_budget(),
                fact_budget: &mut facts(),
                cancelled: || false,
            },
        )
        .unwrap();
        let mut cursor = objects
            .verified_original_cursor(&row.location.original, row.encoded_bytes, io)
            .unwrap();
        let schema = verify_cognition_history_original_record_schema_v1(
            CognitionOriginalCborRequestV1 {
                field: CognitionHistoryFieldV1::Responses,
                id: row.location.original,
                bytes: row.encoded_bytes,
            },
            &mut cursor,
            memory,
            MemoryClass::Ordinary,
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
        cursor.finish().unwrap();
        let joined = bind_indexed_cognition_original_v1(
            point,
            schema,
            &occurrence,
            &selected,
            memory,
            || false,
        )
        .unwrap();
        // A real Recovery join for this exact accepted head must not silently
        // become an Ordinary input simply because the row bytes are fixed.
        let recovery_join = {
            let prior_class = io.class;
            io.class = MemoryClass::Recovery;
            let recovery_state = store
                .state_session(physical(), MemoryClass::Recovery)
                .unwrap();
            let recovery_selected = recover_selected_cognition_history_view_v1(
                &FactRequestV1 {
                    kind: ExecutionFactKindV1::Result,
                    head: &accepted,
                    roots: &sealed.record.after,
                    location: selected.occurrence().location(),
                },
                &mut FactReadContextV1 {
                    originals: &mut objects.recovery_original_reader(io).unwrap(),
                    catalog_pages: &recovery_state,
                    memory,
                    class: MemoryClass::Recovery,
                    core_budget: &mut core_budget(),
                    fact_budget: &mut facts(),
                    cancelled: || false,
                },
            )
            .unwrap();
            let mut recovery_read = WorldFieldReadContextV1 {
                root: after_index.root(),
                state: &recovery_state,
                values: ValueReadContext {
                    source: &values,
                    memory,
                    class: MemoryClass::Recovery,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: 4096,
                record_bytes: 8192,
                take_state_error: || recovery_state.take_read_error().map(state_error),
                cancelled: || false,
            };
            let recovered_index = qualify_cognition_history_index_v1(
                CognitionHistoryFieldV1::Responses,
                selected
                    .manifest()
                    .index_ref(CognitionHistoryFieldV1::Responses),
                &mut recovery_read,
                &mut CognitionHistoryIndexWorkV1 {
                    cells_remaining: 64,
                    max_pending_node_bytes: 1 << 20,
                },
                |_, _| Ok(()),
            )
            .unwrap();
            let point = read_cognition_history_index_point_v1(
                &recovered_index,
                index.unwrap().record_count(),
                &mut recovery_read,
                &mut CognitionHistoryIndexWorkV1 {
                    cells_remaining: 1,
                    max_pending_node_bytes: 1 << 20,
                },
            )
            .unwrap();
            let occurrence = verify_fact_occurrence_v1(
                &FactRequestV1 {
                    kind: ExecutionFactKindV1::Result,
                    head: &accepted,
                    roots: &sealed.record.after,
                    location: row.location,
                },
                &mut FactReadContextV1 {
                    originals: &mut objects.recovery_original_reader(io).unwrap(),
                    catalog_pages: &recovery_state,
                    memory,
                    class: MemoryClass::Recovery,
                    core_budget: &mut core_budget(),
                    fact_budget: &mut facts(),
                    cancelled: || false,
                },
            )
            .unwrap();
            let mut cursor = objects
                .verified_original_cursor(&row.location.original, row.encoded_bytes, io)
                .unwrap();
            let schema = verify_cognition_history_original_record_schema_v1(
                CognitionOriginalCborRequestV1 {
                    field: CognitionHistoryFieldV1::Responses,
                    id: row.location.original,
                    bytes: row.encoded_bytes,
                },
                &mut cursor,
                memory,
                MemoryClass::Recovery,
                &mut CognitionHistoryCborWorkV1 {
                    bytes_remaining: 64 << 10,
                    nodes_remaining: 10000,
                    source_reads_remaining: 100000,
                    key_bytes_remaining: 4096,
                    max_depth: 16,
                    max_key_bytes: 4096,
                },
                || false,
            )
            .unwrap();
            cursor.finish().unwrap();
            let joined = bind_indexed_cognition_original_v1(
                point,
                schema,
                &occurrence,
                &recovery_selected,
                memory,
                || false,
            )
            .unwrap();
            io.class = prior_class;
            joined
        };
        let mut writer = store.value_session(physical());
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
                semantic_bytes: 256,
                payload_bytes: 256,
                record_bytes: 4096,
            },
            cancelled: || false,
        };
        assert!(
            verify_cognition_response_index_successor_v1(
                parent.1,
                &selected,
                index.unwrap(),
                &recovery_join,
                &mut read,
                &mut write,
                &mut CognitionHistoryIndexWorkV1 {
                    cells_remaining: 8,
                    max_pending_node_bytes: 1 << 20
                }
            )
            .is_err(),
            "Recovery original join must be rejected by Ordinary successor"
        );
        let replay = verify_cognition_response_index_successor_v1(
            parent.1,
            &selected,
            index.unwrap(),
            &joined,
            &mut read,
            &mut write,
            &mut CognitionHistoryIndexWorkV1 {
                cells_remaining: 8,
                max_pending_node_bytes: 1 << 20,
            },
        )
        .unwrap();
        assert_eq!(replay.root(), after_index.root());
        assert!(
            verify_cognition_response_index_successor_v1(
                &selected,
                &selected,
                index.unwrap(),
                &joined,
                &mut read,
                &mut write,
                &mut CognitionHistoryIndexWorkV1 {
                    cells_remaining: 8,
                    max_pending_node_bytes: 1 << 20
                }
            )
            .is_err()
        );
    }

    if let Some(proof) = &after_current {
        read.root = sealed.record.after.state_root(StateNamespaceV1::Allocator);
        let binding = bind_cognition_history_current_v1(
            &selected,
            &sealed.record.after,
            CognitionHistoryCurrentSourceV1::Enabled(proof),
            &mut read,
        )
        .unwrap();
        if let Some((life, journal)) = &lifecycle_parent {
            let mut writer = store.value_session(physical());
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
                    payload_bytes: 4096,
                    record_bytes: 8192,
                },
                cancelled: || false,
            };
            if s.alternate_parent.is_none() {
                use oasis7_world_state::incremental_cognition_lifecycle_qualification_v2::{
                    CognitionLifecycleReceiverBuilderV2, CognitionLifecycleReceiverV2,
                };
                // Entry, Formal entry/final inheritance and receiver final
                // cancellation must all refuse a qualification without leaks.
                for boundary in 1..=4 {
                    let used = memory.used();
                    let mut calls = 0;
                    assert!(matches!(
                        CognitionLifecycleReceiverBuilderV2::new(
                            CognitionLifecycleReceiverV2 {
                                extension: &extension,
                                parent: life,
                                before: parent.1,
                                after: &selected,
                                current: &binding,
                                journal,
                            },
                            memory,
                            || {
                                calls += 1;
                                calls == boundary
                            }
                        ),
                        Err(ValueTreeError::Cancelled)
                    ));
                    assert_eq!(memory.used(), used);
                }
            }
            let continued=oasis7_world_state::incremental_cognition_lifecycle_qualification_v2::accept_cognition_lifecycle_appends_v2(
                oasis7_world_state::incremental_cognition_lifecycle_qualification_v2::CognitionLifecycleReceiverV2{extension:&extension,parent:life,before:parent.1,after:&selected,current:&binding,journal},std::iter::empty(),&mut read,&mut write,
                &mut oasis7_world_state::incremental_cognition_lifecycle_index_v2::CognitionLifecycleWorkV2{key_bytes_remaining:0,cells_remaining:0,max_pending_node_bytes:1<<20},
                &mut CognitionHistoryIndexWorkV1{cells_remaining:0,max_pending_node_bytes:1<<20});
            if s.alternate_parent.is_some() {
                assert!(
                    continued.is_err(),
                    "unrelated accepted parent must not inherit lifecycle qualification"
                );
                return (accepted, sealed.record.after.clone(), manifest_id.unwrap());
            }
            let continued = continued.unwrap();
            assert_eq!(continued.keys(), 0);
            assert_eq!(continued.journal_records(), 0);
            assert_eq!(continued.root(), life.root());
        }
    }
    (accepted, sealed.record.after.clone(), manifest_id.unwrap())
}
