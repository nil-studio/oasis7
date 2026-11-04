//! Two real Journal events in one signed ordinary transition. This verifies
//! per-key lifecycle and index acceptance, not complete business HistoryReady.
use super::signed::{core_budget, facts, put};
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal,
    incremental_execution::{
        ExecutionFactKindV1, ExecutionFactRefV1, ExecutionParent, ExecutionTransaction,
        TransactionLimits, TransitionVerifier,
    },
    incremental_history::{CommitBodyV1, HeaderPreparation, HistoryPrefixV1},
    incremental_material::{
        FactLocationV1, FactReadContextV1, FactRequestV1, verify_fact_occurrence_v1,
    },
    incremental_record_root::RecordRootRefV1,
    incremental_state::StateNamespaceV1,
};
use oasis7_world_state::{
    incremental_cognition_control_overlay::{CognitionControlValueV1, change_cognition_control_v1},
    incremental_cognition_fields::CognitionControlFieldV1,
    incremental_cognition_history_cbor::*,
    incremental_cognition_history_hash_outboard::*,
    incremental_cognition_lifecycle_index_v2::*,
    incremental_cognition_lifecycle_qualification_v2::*,
    incremental_field_overlay::WorldFieldOverlayContextV1,
};
pub(super) struct JournalFrame<'a> {
    pub source: normal::Source<'a>,
    pub head: &'a formal::VerifiedFormalCommitV1,
    pub view: &'a oasis7_proto::memory_budget::ReservedValue<AcceptedCognitionHistoryViewV1>,
}
fn work() -> CognitionLifecycleWorkV2 {
    CognitionLifecycleWorkV2 {
        key_bytes_remaining: 1 << 20,
        cells_remaining: 100,
        max_pending_node_bytes: 1 << 20,
    }
}
fn index_work() -> CognitionHistoryIndexWorkV1 {
    CognitionHistoryIndexWorkV1 {
        cells_remaining: 64,
        max_pending_node_bytes: 1 << 20,
    }
}
fn hash_work() -> CognitionHeadHashWorkV1 {
    CognitionHeadHashWorkV1 {
        input_bytes_remaining: 16 << 20,
        hash_nodes_remaining: 1000000,
    }
}
fn cbor_work() -> CognitionHistoryCborWorkV1 {
    CognitionHistoryCborWorkV1 {
        bytes_remaining: 1 << 20,
        nodes_remaining: 100000,
        source_reads_remaining: 100000,
        key_bytes_remaining: 1 << 20,
        max_depth: 16,
        max_key_bytes: 4096,
    }
}
pub(super) fn append_two_events(
    frame: JournalFrame<'_>,
    store: &IncrementalPageStore,
    objects: &IncrementalObjectStore<'_>,
    io: &mut ObjectIo<'_>,
    corrupt_source: bool,
) {
    let source = frame.source;
    let memory = source.memory;
    let view = frame.view;
    let _fixture = memory.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
    let body_bytes = objects
        .read_object_owned(&view.occurrence().head_body_id(), 4096, io)
        .unwrap();
    let body = CommitBodyV1::from_canonical_bytes(&body_bytes).unwrap();
    let prefix_bytes = objects.read_object_owned(&body.history, 4096, io).unwrap();
    let prefix = HistoryPrefixV1::from_canonical_bytes(&prefix_bytes).unwrap();
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut writer = store.value_session(physical());
    let mut read_io = logical();
    let mut read_keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut read = WorldFieldReadContextV1 {
        root: source.roots.state_root(StateNamespaceV1::Allocator),
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
    let current = enabled::qualify_current(source.roots, store, memory, MemoryClass::Ordinary);
    let parent_binding = bind_cognition_history_current_v1(
        view,
        source.roots,
        CognitionHistoryCurrentSourceV1::Enabled(&current),
        &mut read,
    )
    .unwrap();
    let parent_journal = view
        .manifest()
        .index_ref(CognitionHistoryFieldV1::JournalEvents);
    read.root = &parent_journal.root;
    let journal = qualify_cognition_history_index_v1(
        CognitionHistoryFieldV1::JournalEvents,
        parent_journal,
        &mut read,
        &mut index_work(),
        |_, _| panic!("empty genesis Journal"),
    )
    .unwrap();
    let lifecycle =
        qualify_empty_cognition_lifecycle_v2(view, &parent_binding, &journal, memory, || false)
            .unwrap();
    let cache = recover_cognition_head_cache_v1(
        view.derived().unwrap().caches[0].unwrap(),
        &mut read.values,
        &mut hash_work(),
        || false,
    )
    .unwrap();
    let mut builder =
        CognitionHeadHashBuilderV1::append_to(cache.state(), memory, MemoryClass::Ordinary, || {
            false
        })
        .unwrap();
    drop(cache);
    let catalog = body
        .catalog_before
        .extend(&body, &state, memory, 4 << 20)
        .unwrap();
    let parent = ExecutionParent {
        roots: source.roots.clone(),
        commit: body.digest().unwrap(),
        authority: body.authority,
    };
    let fence = parent.fence().unwrap();
    let verification_parent = ExecutionParent {
        roots: parent.roots.clone(),
        commit: parent.commit,
        authority: parent.authority,
    };
    let mut tx = ExecutionTransaction::begin(
        parent,
        [61; 32],
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
    let input = *blake3::hash(b"actual Journal delta input").as_bytes();
    put(objects, b"actual Journal delta input", io);
    tx.append_fact(ExecutionFactRefV1 {
        kind: ExecutionFactKindV1::Input,
        id: input,
    })
    .unwrap();
    let mut write_io = logical();
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let cancel = std::cell::Cell::new(false);
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
        cancelled: || cancel.get(),
    };
    let keys = ["agent", "session", "turn", "decision", "request"];
    let first = super::lifecycle_physical::event("TurnStarted", 1, 0, "", keys);
    let second = super::lifecycle_physical::event(
        "ContextCaptured",
        2,
        0,
        first["event_digest"].as_str().unwrap(),
        keys,
    );
    let events = [first, second];
    let mut index_after = None;
    let mut life_root = lifecycle.root().clone();
    let mut life_keys = 0;
    let mut source_last = None;
    let mut originals = CognitionHistoryWriteContextV1::new(
        memory,
        CanonicalCborWorkV1 {
            visits_remaining: 100000,
            bytes_remaining: 16 << 20,
            max_depth: 16,
            max_map_keys: 64,
            max_key_bytes: 4096,
        },
        2,
        || false,
    );
    for (ordinal, event) in events.iter().enumerate() {
        let guard = tx.guard();
        let stored = contribute_cognition_history_original_v1(
            CognitionHistoryRecordRefV1::from_original(
                CognitionHistoryFieldV1::JournalEvents,
                ordinal as u64,
                event,
            )
            .unwrap(),
            &mut tx,
            &mut originals,
            &mut CognitionHistoryObjectSinkV1 {
                objects,
                io,
                cancelled: || false,
            },
            &guard,
            &fence,
        )
        .unwrap();
        let location = FactLocationV1 {
            original: *stored.id(),
            height: source.roots.height + 1,
            ordinal: tx.ordered_fact_count() as u64 - 1,
        };
        let next = prepare_cognition_history_index_append_v1(
            index_after.as_ref().unwrap_or(&journal),
            CognitionHistoryIndexRowV1 {
                location,
                encoded_bytes: stored.encoded_bytes(),
            },
            &mut read,
            &mut write,
            &mut index_work(),
        )
        .unwrap();
        index_after = Some(next);
        let mut cursor = objects
            .verified_original_cursor(stored.id(), stored.encoded_bytes(), io)
            .unwrap();
        let schema = verify_cognition_history_original_record_schema_v1(
            CognitionOriginalCborRequestV1 {
                field: CognitionHistoryFieldV1::JournalEvents,
                id: *stored.id(),
                bytes: stored.encoded_bytes(),
            },
            &mut cursor,
            memory,
            MemoryClass::Ordinary,
            &mut cbor_work(),
            || false,
        )
        .unwrap();
        cursor.finish().unwrap();
        let CognitionOriginalIdentityStatusV1::DigestMatched(semantic_digest) =
            schema.semantic().identity_status()
        else {
            panic!("real event digest");
        };
        source_last = Some(CognitionHeadSourceLastV3 {
            sequence: ordinal as u64 + 1,
            semantic_digest,
            location,
            original_bytes: stored.encoded_bytes(),
        });
        let prepared = prepare_cognition_lifecycle_append_v2(
            &life_root,
            life_keys,
            CognitionLifecycleAppendV2 {
                schema: &schema,
                keys,
                ordinal: ordinal as u64,
                location,
            },
            &mut read,
            &mut write,
            &mut work(),
        )
        .unwrap();
        life_root = prepared.root;
        life_keys = prepared.keys;
        let mut cache_event = event.clone();
        if corrupt_source {
            cache_event["world_id"] = "other-world".into();
        }
        let encoded = oasis7_wasm_abi::encode_canonical_cbor(&cache_event).unwrap();
        builder
            .append(&encoded, &mut hash_work(), || false)
            .unwrap();
    }
    let cache = builder.finish(|| false).unwrap();
    let digest = cache
        .digest(CognitionHeadHashV1::Journal, 2, 2, &mut hash_work(), || {
            false
        })
        .unwrap();
    let mut oracle_events = events.clone();
    if corrupt_source {
        for event in &mut oracle_events {
            event["world_id"] = "other-world".into();
        }
    }
    let original = oasis7_world_state::cognition_recovery::cognition_digest_v1(
        "oasis7.cognition.journal-head.v1",
        &serde_json::json!({"head_seq":2,"events":oracle_events}),
    );
    assert_eq!(
        format!("blake3:{}", blake3::Hash::from(digest).to_hex()),
        original
    );
    drop(events);
    let cache_id = persist_cognition_head_cache_v1(
        &cache,
        &mut *write.sink,
        memory,
        &mut *write.io,
        &mut hash_work(),
        || false,
    )
    .unwrap();
    struct CacheDigest(blake3::Hasher);
    impl std::io::Write for CacheDigest {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut cache_digest = CacheDigest(blake3::Hasher::new());
    cache
        .write_canonical_cache_v1(&mut cache_digest, &mut hash_work(), || false)
        .unwrap();
    let cache_digest = *cache_digest.0.finalize().as_bytes();
    drop(cache);
    let before_digest = format!(
        "blake3:{}",
        blake3::Hash::from(view.manifest().heads[0].digest.unwrap()).to_hex()
    );
    let after_digest = format!("blake3:{}", blake3::Hash::from(digest).to_hex());
    for (field, change) in [
        (
            CognitionControlFieldV1::JournalHeadSeq,
            (
                CognitionControlValueV1::Unsigned(0),
                CognitionControlValueV1::Unsigned(2),
            ),
        ),
        (
            CognitionControlFieldV1::JournalHeadDigest,
            (
                CognitionControlValueV1::Text(before_digest.as_str()),
                CognitionControlValueV1::Text(after_digest.as_str()),
            ),
        ),
    ] {
        let guard = tx.guard();
        change_cognition_control_v1(
            field,
            change,
            &mut WorldFieldOverlayContextV1 {
                transaction: &mut tx,
                values: ValueReadContext {
                    source: &values,
                    memory,
                    class: MemoryClass::Ordinary,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: 4096,
                record_bytes: 8192,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            },
            &mut write,
            &guard,
            &fence,
        )
        .unwrap();
    }
    let guard = tx.guard();
    let mut manifest_id = None;
    let sealed=tx.seal_with_final_fact(&guard,&fence,source.roots.simulation_tick+1,||false,|after|{
  let mut manifest=view.manifest().clone();manifest.roots=after.digest().unwrap();manifest.revision+=1;manifest.previous_view=Some(view.occurrence().location());manifest.heads[0]=CognitionHistoryHeadV1{sequence:2,digest:Some(digest)};
  let index=index_after.as_ref().unwrap();manifest.indexes[0].root=index.root().clone();manifest.indexes[0].records=index.record_count();let mut refs=view.derived().unwrap().clone();refs.caches[0]=Some(cache_id);refs.lifecycle=Some(RecordRootRefV1::new(lifecycle_schema(),lifecycle_owner(),life_root.clone()).unwrap());
  let fact=if let Some(parent_source)=view.source_v3() {
    let mut summary=(***parent_source).clone();summary.mode=CognitionHistorySourceModeV3::Delta;summary.cache_state_digests[0]=Some(cache_digest);summary.last[0]=source_last;summary.lifecycle_keys=life_keys;summary.lifecycle_journal_records=index.record_count();
    super::source_v3::persist_v3_summary(&manifest,&refs,&summary,memory,objects,io,||false)?
  }else{oasis7_chain_runtime::incremental_cognition_history_store::persist_cognition_history_view_result_v2(after,&manifest,&refs,memory,objects,io,||false)?};manifest_id=Some(fact.id);Ok::<_,ValueTreeError>(fact)
 }).unwrap();
    read.root = sealed.record.before.state_root(StateNamespaceV1::Allocator);
    let after_current =
        oasis7_world_state::incremental_cognition_family::accept_cognition_current_transition_v1(
            &current,
            &TransitionVerifier {
                parent: &verification_parent,
                operation: [61; 32],
                source: &state,
                memory,
                class: MemoryClass::Ordinary,
                max_pending_node_bytes: 4 << 20,
            },
            &sealed.record,
            &mut read,
            &mut oasis7_world_state::incremental_json_records::JsonScalarChangeWorkV1 {
                paths_remaining: 2,
                max_pending_pairs: 0,
                json: oasis7_world_state::incremental_json_records::JsonReadWorkV1 {
                    cells_remaining: 0,
                    json_nodes_remaining: 0,
                    max_depth: 8,
                    max_payload_bytes: 1 << 20,
                },
            },
        )
        .unwrap();
    // Core verification reopens the actual after-state; publish every prepared
    // transition page, just as the production commit sink does.
    {
        use oasis7_formal_protocol::incremental_value::ValueNodeWriter;
        let mut no_cancel = || false;
        let mut pages = store
            .buffered_page_writer(io.physical, io.cancelled, &mut no_cancel)
            .unwrap();
        for node in &sealed.new_nodes {
            let charge = memory
                .reserve(node.bytes.capacity() + 128, MemoryClass::Ordinary)
                .unwrap();
            pages
                .put_node(
                    *blake3::hash(&node.bytes).as_bytes(),
                    charge.retain(node.bytes.clone()),
                )
                .unwrap();
        }
        pages.finish().unwrap();
    }
    let history = HeaderPreparation {
        config: source.config,
        catalog: &catalog.after,
        roots: &sealed.record.after,
        parent: Some(&body),
        parent_history: Some(&prefix),
        transition: Some(&sealed.record),
        genesis_facts: &[],
        operation: [61; 32],
        authority: body.authority,
        execution_environment: source.config.execution_environment,
        checkpoint: false,
    }
    .prepare(memory)
    .unwrap();
    let (accepted, selected, extension, checked_complete) =
        super::normal_accept::accept_normal_history_with_candidate(
            super::normal_accept::NormalHistoryFrame {
                authority: source.authority,
                parent: frame.head,
                after: &sealed.record.after,
                history: &history,
                input,
                expected_facts: 4,
            },
            memory,
            store,
            objects,
            io,
            |candidate, core, originals, _checked, io| {
                view.source_v3().map(|_| {
                    super::source_v3_ordinary::check_ordinary(
                        super::source_v3_ordinary::OrdinaryFrame {
                            before: view,
                            before_current: &parent_binding,
                            before_journal: &journal,
                            before_lifecycle: &lifecycle,
                            after_current: &after_current,
                            after_journal: index_after.as_ref().unwrap(),
                        },
                        &CognitionHistorySelectionRequestV1 {
                            candidate,
                            core,
                            originals,
                        },
                        store,
                        objects,
                        io,
                        memory,
                    )
                })
            },
        );
    let _promoted_complete = checked_complete.as_ref().map(|checked| {
        promote_complete_cognition_history_source_v3(checked, &accepted, memory, || false).unwrap()
    });
    read.root = sealed.record.after.state_root(StateNamespaceV1::Allocator);
    let binding = bind_cognition_history_current_v1(
        &selected,
        &sealed.record.after,
        CognitionHistoryCurrentSourceV1::Enabled(&after_current),
        &mut read,
    )
    .unwrap();
    let request = || CognitionLifecycleReceiverV2 {
        extension: &extension,
        parent: &lifecycle,
        before: view,
        after: &selected,
        current: &binding,
        journal: &journal,
    };
    let mut receiver =
        CognitionLifecycleReceiverBuilderV2::new(request(), memory, || false).unwrap();
    let mut poisoned =
        CognitionLifecycleReceiverBuilderV2::new(request(), memory, || false).unwrap();
    let mut cancelled =
        CognitionLifecycleReceiverBuilderV2::new(request(), memory, || false).unwrap();
    let mut actual_source =
        AuthenticatedHeadStreamV1::new(&selected, index_after.as_ref().unwrap(), memory, || false)
            .unwrap();
    let mut last = None;
    for ordinal in 0..2 {
        let index = index_after.as_ref().unwrap();
        read.root = index.root();
        let point =
            read_cognition_history_index_point_v1(index, ordinal, &mut read, &mut index_work())
                .unwrap();
        let row = point.row();
        let occurrence = verify_fact_occurrence_v1(
            &FactRequestV1 {
                kind: ExecutionFactKindV1::Event,
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
                field: CognitionHistoryFieldV1::JournalEvents,
                id: row.location.original,
                bytes: row.encoded_bytes,
            },
            &mut cursor,
            memory,
            MemoryClass::Ordinary,
            &mut cbor_work(),
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
        let mut source_cursor = objects
            .verified_original_cursor(&row.location.original, row.encoded_bytes, io)
            .unwrap();
        actual_source
            .push(
                &joined,
                &selected,
                &mut source_cursor,
                memory,
                &mut cbor_work(),
                &mut hash_work(),
                || false,
            )
            .unwrap();
        source_cursor.finish().unwrap();
        receiver
            .push(
                &joined,
                keys,
                &mut read,
                &mut write,
                &mut work(),
                &mut index_work(),
            )
            .unwrap();
        poisoned
            .push(
                &joined,
                keys,
                &mut read,
                &mut write,
                &mut work(),
                &mut index_work(),
            )
            .unwrap();
        cancelled
            .push(
                &joined,
                keys,
                &mut read,
                &mut write,
                &mut work(),
                &mut index_work(),
            )
            .unwrap();
        last = Some(joined);
    }
    let persisted = recover_cognition_head_cache_v1(
        selected.derived().unwrap().caches[0].unwrap(),
        &mut read.values,
        &mut hash_work(),
        || false,
    )
    .unwrap();
    // Even the adversarial cache is a real accepted, mathematically bound
    // fixed frontier. Its source is independently checked below.
    let mathematical = bind_persisted_cognition_head_caches_v2(
        [Some(persisted), None, None],
        &selected,
        &binding,
        memory,
        &mut hash_work(),
        || false,
    )
    .unwrap();
    let actual_source = actual_source.finish(&selected, memory, &mut hash_work(), || false);
    if corrupt_source {
        assert!(matches!(
            actual_source,
            Err(ValueTreeError::Invalid("authenticated head stream digest"))
        ));
    } else {
        AuthenticatedCognitionHeadSourceV1::validate_persisted_cache(
            &actual_source.unwrap(),
            mathematical.cache(0).unwrap(),
            &selected,
            memory,
            &mut hash_work(),
            || false,
        )
        .unwrap();
    }
    let qualified = receiver.finish(&mut read, &mut write).unwrap();
    assert_eq!(qualified.keys(), 1);
    assert_eq!(qualified.journal_records(), 2);
    assert_eq!(qualified.root(), &life_root);
    assert!(
        poisoned
            .push(
                last.as_ref().unwrap(),
                keys,
                &mut read,
                &mut write,
                &mut work(),
                &mut index_work()
            )
            .is_err()
    );
    assert!(poisoned.finish(&mut read, &mut write).is_err());
    cancel.set(true);
    assert!(matches!(
        cancelled.finish(&mut read, &mut write),
        Err(MemberRecordError::Value(ValueTreeError::Cancelled))
    ));
    cancel.set(false);
    assert_eq!(selected.manifest().heads[0].sequence, 2);
    assert_eq!(
        selected.occurrence().location().original,
        manifest_id.unwrap()
    );
}
