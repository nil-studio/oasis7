#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::*;
use oasis7_formal_protocol::{incremental_state::*, incremental_value::*};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_runtime::{
    incremental_cognition_fields::*, incremental_cognition_genesis::*,
    incremental_cognition_json::*, incremental_cognition_json_read::*,
    incremental_cognition_read::*, incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
};
#[path = "support/incremental_cognition_controls.rs"]
mod cognition_controls;
#[path = "support/incremental_cognition_json_lifecycle.rs"]
mod cognition_json_lifecycle;
#[path = "support/incremental_cognition_missing_child.rs"]
mod cognition_missing_child;
#[path = "support/incremental_cognition_namespace.rs"]
mod cognition_namespace;
#[path = "support/incremental_cognition_pages.rs"]
mod cognition_pages;
#[path = "support/incremental_cognition_family_faults.rs"]
mod family_faults;
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 2_500_000,
        read_bytes_remaining: 256 << 20,
        writes_remaining: 100_000,
        write_bytes_remaining: 256 << 20,
    }
}
fn source(all_optional: bool) -> serde_json::Value {
    let mut value = serde_json::json!({"schema_version":"cognition-persistence.v1", "cognition_journal":{"schema_version":"cognition-journal.v1","head_seq":u64::MAX,"head_digest":"原digest","events":[]}, "responses":[],"commit_records":[],"receipt_registry":[],"receipt_lineage_registry":[],"feedback_outbox":{"":true},"idempotency_index":{},"staged_actions":{},"scheduler_state":{},"continuations":[null,""],"recovery":null,"continuation_contexts":{},"reorg_invalidation_epoch":0});
    if all_optional {
        value["cognition_economy"] =
            serde_json::to_value(oasis7_runtime::CognitionEconomyStateV1::default()).unwrap();
        value["cognition_economy"]["balances"] =
            serde_json::json!({"":{}, "账户":{"unit":{"available":u64::MAX,"reserved":0}}});
        value["cognition_economy"]["provisioned_balances"] = serde_json::json!({"原binding":{}});
        value["cognition_economy"]["provision_head_seq"] = serde_json::json!(42);
        value["cognition_economy"]["head_seq"] = serde_json::json!(u64::MAX);
        value["cognition_economy"]["head_digest"] = serde_json::json!("原economic-digest");
        value["cognition_economy"]["provision_head_digest"] = serde_json::json!("");
        value["agent_delegation"] = serde_json::json!({"grants":{"outer-id":{"grant_id":"embedded-id"}},"spent":{"":u64::MAX},"decisions":{},"receipts":{},"controls":{}});
        value["continuation_evaluations"] = serde_json::json!({"n":{"tick":u64::MAX}});
    }
    value
}
fn state_error(error: PageStoreError) -> ValueTreeError {
    match error {
        PageStoreError::Memory(error) => ValueTreeError::Memory(error),
        PageStoreError::ResourceLimited(error) => ValueTreeError::ResourceLimited(error),
        PageStoreError::Cancelled => ValueTreeError::Cancelled,
        error => ValueTreeError::Io(std::io::Error::other(error)),
    }
}
#[test]
fn cognition_actual_gen_reopens_control_and_json_entries_without_snapshot_or_old_pool() {
    run_cognition(false);
}
#[test]
fn cognition_optional_economy_and_delegation_keep_original_state_and_side_routing() {
    run_cognition(true);
}
fn run_cognition(all_optional: bool) {
    let directory = fixture::Directory::new();
    let path = directory.0.join("cognition");
    let namespaces = [
        StateNamespaceV1::Artifacts,
        StateNamespaceV1::Allocator,
        StateNamespaceV1::Effects,
        StateNamespaceV1::Authorization,
        StateNamespaceV1::Scheduler,
        StateNamespaceV1::Business,
    ];
    let mut roots = [[0u8; STATE_ROOT_BYTES]; 6];
    let expected_feedback;
    {
        let memory = MemoryBudget::new(32 << 20, 8 << 20).unwrap();
        let store =
            IncrementalPageStore::create(&path, memory.clone(), PageCompression::Raw).unwrap();
        let _input = memory.reserve(16 << 10, MemoryClass::Ordinary).unwrap();
        let source = source(all_optional);
        let _batch = memory.reserve(32 << 10, MemoryClass::Ordinary).unwrap();
        let mut writes = Vec::with_capacity(64);
        let mut writer = store.value_session(PageIoBudget {
            writes_remaining: if all_optional { 200_000 } else { 100_000 },
            ..physical()
        });
        let mut io = fixture::logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut records = 64;
        let mut ctx = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut io,
            key_work: &mut keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 4096,
                record_bytes: 8192,
            },
            cancelled: || false,
        };
        prepare_cognition_controls_genesis_v1(
            CognitionEnvelopeRefV1::new(&source).unwrap(),
            &mut ctx,
            &mut records,
            &mut |write| {
                assert!(writes.len() < 64);
                writes.push(write);
                Ok(())
            },
        )
        .unwrap();
        let mut json_work = oasis7_runtime::incremental_json_records::JsonWriteWorkV1 {
            cells_remaining: 4096,
            json_nodes_remaining: 512,
            max_json_depth: 4,
            max_pending_node_bytes: 2 << 20,
        };
        let generated = prepare_cognition_json_genesis_v1(
            CognitionEnvelopeRefV1::new(&source).unwrap(),
            &mut ctx,
            &mut records,
            &mut json_work,
            &mut |write| {
                assert!(writes.len() < 64);
                writes.push(write);
                Ok(())
            },
        );
        eprintln!(
            "Cognition GEN all_optional={all_optional} records_left={records} cells_left={} json_nodes_left={} writer_remaining={:?} logical_remaining={:?}",
            json_work.cells_remaining,
            json_work.json_nodes_remaining,
            writer.remaining_io().unwrap(),
            io
        );
        generated.unwrap();
        assert_eq!(writes.len(), if all_optional { 60 } else { 22 });
        assert_eq!(records, if all_optional { 4 } else { 42 });
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        for (index, namespace) in namespaces.into_iter().enumerate() {
            let count = writes.iter().filter(|w| w.namespace == namespace).count();
            let _heap = memory
                .reserve(count * (512 << 10) + (128 << 10), MemoryClass::Ordinary)
                .unwrap();
            let mut changes = Vec::with_capacity(count);
            for write in writes.iter().filter(|w| w.namespace == namespace) {
                changes.push(StateWriteV1 {
                    key: write.mutation.key.clone(),
                    before: None,
                    after: write.mutation.after,
                });
            }
            changes.sort_by(|a, b| a.key.cmp(&b.key));
            let delta = _heap.retain(
                StateDeltaV1::build(
                    StateRootV1::empty(namespace).unwrap(),
                    changes,
                    &state,
                    2 << 20,
                )
                .unwrap(),
            );
            assert_eq!(delta.after.leaf_count, count as u64);
            writer
                .put_state_pages_cancelled(&delta, &memory, &mut || false)
                .unwrap();
            roots[index].copy_from_slice(&delta.after.canonical_bytes().unwrap());
        }
        // The neutral point producer changes one scalar below a known qualified
        // JSON graph. It does not publish the Cognition outer root or grant family
        // qualification; the cold test below intentionally reopens the old root.
        {
            use oasis7_runtime::incremental_json_records::*;
            let _temporary = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
            let root = StateRootV1::from_canonical_bytes(&roots[2]).unwrap();
            let reader = store.value_session(physical());
            let mut read_io = fixture::logical();
            let mut read_keys = SemanticKeyBudget {
                bytes_remaining: 16 << 20,
            };
            let mut read = WorldFieldReadContextV1 {
                root: &root,
                state: &state,
                values: ValueReadContext {
                    source: &reader,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut read_io,
                },
                key_work: &mut read_keys,
                semantic_bytes: 4096,
                record_bytes: 8192,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            };
            let mut full = JsonReadWorkV1 {
                cells_remaining: 128,
                json_nodes_remaining: 24,
                max_depth: 4,
                max_payload_bytes: 4096,
            };
            let entry = qualify_cognition_json_entry_v1(
                CognitionJsonFieldV1::FeedbackOutbox,
                &mut read,
                &mut full,
            )
            .unwrap()
            .unwrap();
            let parent = qualify_json_subtree_v1(
                CognitionJsonFieldV1::FeedbackOutbox.profile(),
                entry.body_root(),
                &mut read,
                &mut full,
            )
            .unwrap();
            cognition_namespace::reject_relabelled_member(&store, &memory, parent.root());
            if !all_optional {
                cognition_json_lifecycle::exercise(&store, &memory, &parent);
            }
            {
                // The original proof's retained carrier belongs to `memory`, not
                // to a new writer/reader pool reopening the same authenticated pages.
                let other = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
                assert!(!parent.uses_budget(&other));
                let other_store = IncrementalPageStore::open(
                    &path,
                    other.clone(),
                    PageCompression::Raw,
                    MemoryClass::Ordinary,
                )
                .unwrap();
                let other_state = other_store
                    .state_session(physical(), MemoryClass::Ordinary)
                    .unwrap();
                let other_reader = other_store.value_session(physical());
                let mut other_writer = other_store.value_session(physical());
                let mut read_io = fixture::logical();
                let mut write_io = fixture::logical();
                let mut read_keys = SemanticKeyBudget {
                    bytes_remaining: 16 << 20,
                };
                let mut write_keys = SemanticKeyBudget {
                    bytes_remaining: 16 << 20,
                };
                let mut other_read = WorldFieldReadContextV1 {
                    root: &root,
                    state: &other_state,
                    values: ValueReadContext {
                        source: &other_reader,
                        memory: &other,
                        class: MemoryClass::Ordinary,
                        io: &mut read_io,
                    },
                    key_work: &mut read_keys,
                    semantic_bytes: 4096,
                    record_bytes: 8192,
                    take_state_error: || other_state.take_read_error().map(state_error),
                    cancelled: || false,
                };
                let mut other_write = MemberRecordWriteContext {
                    sink: &mut other_writer,
                    memory: &other,
                    io: &mut write_io,
                    key_work: &mut write_keys,
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 4096,
                        payload_bytes: 4096,
                        record_bytes: 8192,
                    },
                    cancelled: || false,
                };
                let result = prepare_json_scalar_path_change_v1(
                    CognitionJsonFieldV1::FeedbackOutbox.profile(),
                    &parent,
                    &[JsonPathSegmentV1::Name("")],
                    (JsonScalarRefV1::Bool(true), JsonScalarRefV1::Unsigned(7)),
                    &mut other_read,
                    &mut other_write,
                    &mut JsonWriteWorkV1 {
                        cells_remaining: 32,
                        json_nodes_remaining: 8,
                        max_json_depth: 4,
                        max_pending_node_bytes: 2 << 20,
                    },
                );
                assert!(
                    matches!(
                        result,
                        Err(MemberRecordError::Value(ValueTreeError::Invalid(_)))
                    ),
                    "foreign parent carrier must fail before provisional success: {result:?}"
                );
                assert_eq!(
                    other_writer.remaining_io().unwrap().writes_remaining,
                    physical().writes_remaining
                );
                assert_eq!(
                    other_reader.remaining_io().unwrap().reads_remaining,
                    physical().reads_remaining
                );
            }
            let mut write_io = fixture::logical();
            let mut write_keys = SemanticKeyBudget {
                bytes_remaining: 16 << 20,
            };
            let mut write = MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut write_io,
                key_work: &mut write_keys,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 4096,
                    payload_bytes: 4096,
                    record_bytes: 8192,
                },
                cancelled: || false,
            };
            let mut point_work = JsonWriteWorkV1 {
                cells_remaining: 32,
                json_nodes_remaining: 8,
                max_json_depth: 4,
                max_pending_node_bytes: 2 << 20,
            };
            let changed = prepare_json_scalar_path_change_v1(
                CognitionJsonFieldV1::FeedbackOutbox.profile(),
                &parent,
                &[JsonPathSegmentV1::Name("")],
                (
                    JsonScalarRefV1::Bool(true),
                    JsonScalarRefV1::Text("原changed"),
                ),
                &mut read,
                &mut write,
                &mut point_work,
            )
            .unwrap();
            assert!(changed.changed());
            assert_eq!(changed.before(), parent.root());
            assert_ne!(changed.before(), changed.after());
            assert_eq!(changed.before().leaf_count, changed.after().leaf_count);
            assert!(matches!(
                advance_json_scalar_subtree_v1(
                    CognitionJsonFieldV1::FeedbackOutbox.profile(),
                    &parent,
                    changed.after(),
                    &mut read,
                    &mut JsonScalarChangeWorkV1 {
                        json: JsonReadWorkV1 {
                            cells_remaining: 0,
                            json_nodes_remaining: 8,
                            max_depth: 4,
                            max_payload_bytes: 4096
                        },
                        paths_remaining: 4096,
                        max_pending_pairs: 8
                    }
                ),
                Err(ValueTreeError::ResourceLimited("JSON scalar fixed cells"))
            ));
            let advanced = advance_json_scalar_subtree_v1(
                CognitionJsonFieldV1::FeedbackOutbox.profile(),
                &parent,
                changed.after(),
                &mut read,
                &mut JsonScalarChangeWorkV1 {
                    json: JsonReadWorkV1 {
                        cells_remaining: 64,
                        json_nodes_remaining: 8,
                        max_depth: 4,
                        max_payload_bytes: 4096,
                    },
                    paths_remaining: 4096,
                    max_pending_pairs: 8,
                },
            )
            .unwrap();
            assert_eq!(parent.exact_node_count(), Some(2));
            assert_eq!(advanced.exact_node_count(), parent.exact_node_count());
            assert_eq!(
                advanced.node_count_upper_bound(),
                parent.node_count_upper_bound()
            );
            let after = qualify_json_subtree_v1(
                CognitionJsonFieldV1::FeedbackOutbox.profile(),
                changed.after(),
                &mut read,
                &mut full,
            )
            .unwrap();
            assert_eq!(advanced.root(), after.root());
            let noop = prepare_json_scalar_path_change_v1(
                CognitionJsonFieldV1::FeedbackOutbox.profile(),
                &advanced,
                &[JsonPathSegmentV1::Name("")],
                (
                    JsonScalarRefV1::Text("原changed"),
                    JsonScalarRefV1::Text("原changed"),
                ),
                &mut read,
                &mut write,
                &mut point_work,
            )
            .unwrap();
            assert!(!noop.changed());
            assert_eq!(noop.before(), noop.after());
            // Actual shared transaction: coalesced same-entry updates, ordinary
            // JSON receiver, generic replay, then durable namespace pages.
            use oasis7_formal_protocol::incremental_execution::*;
            use oasis7_runtime::{
                incremental_cognition_json_overlay::*, incremental_cognition_json_receiver::*,
                incremental_field_overlay::WorldFieldOverlayContextV1,
            };
            let mut top = RootSetV1::empty("cognition-test".into(), [0x52; 32]).unwrap();
            for (index, namespace) in namespaces.iter().enumerate() {
                *top.state
                    .iter_mut()
                    .find(|r| r.namespace == *namespace)
                    .unwrap() = StateRootV1::from_canonical_bytes(&roots[index]).unwrap();
            }
            let parent_family =
                (!all_optional).then(|| family_faults::qualify_ordinary(&store, &memory, &top));
            let execution_parent = ExecutionParent {
                roots: top,
                commit: [4; 32],
                authority: AuthorityBinding {
                    token: [5; 32],
                    epoch: 7,
                },
            };
            if let Some(parent_family) = &parent_family {
                cognition_controls::exercise(&store, &memory, &execution_parent, parent_family);
                cognition_json_lifecycle::same_transaction(
                    &store,
                    &memory,
                    &execution_parent,
                    parent_family,
                );
            }
            let live = execution_parent.fence().unwrap();
            let mut tx = ExecutionTransaction::begin(
                execution_parent.clone(),
                [6; 32],
                &state,
                &memory,
                TransactionLimits {
                    max_reads: 8,
                    max_writes: 2,
                    max_facts: 0,
                    max_pending_node_bytes: 2 << 20,
                },
            )
            .unwrap();
            let mut overlay_io = fixture::logical();
            let mut overlay_keys = SemanticKeyBudget {
                bytes_remaining: 16 << 20,
            };
            let mut overlay = WorldFieldOverlayContextV1 {
                transaction: &mut tx,
                values: ValueReadContext {
                    source: &reader,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut overlay_io,
                },
                key_work: &mut overlay_keys,
                semantic_bytes: 4096,
                record_bytes: 8192,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            };
            let mut owner_work = CognitionJsonPointWorkV1 {
                point: JsonWriteWorkV1 {
                    cells_remaining: 96,
                    json_nodes_remaining: 24,
                    max_json_depth: 4,
                    max_pending_node_bytes: 2 << 20,
                },
                changed: JsonScalarChangeWorkV1 {
                    json: JsonReadWorkV1 {
                        cells_remaining: 128,
                        json_nodes_remaining: 24,
                        max_depth: 4,
                        max_payload_bytes: 4096,
                    },
                    paths_remaining: 16384,
                    max_pending_pairs: 8,
                },
            };
            let guard = overlay.transaction.guard();
            let first = change_cognition_json_scalar_v1(
                CognitionJsonScalarChangeV1 {
                    field: CognitionJsonFieldV1::FeedbackOutbox,
                    qualification: &entry,
                    path: &[JsonPathSegmentV1::Name("")],
                    change: (JsonScalarRefV1::Bool(true), JsonScalarRefV1::Unsigned(9)),
                },
                &mut overlay,
                &mut write,
                &mut owner_work,
                &guard,
                &live,
            )
            .unwrap()
            .unwrap();
            let guard = overlay.transaction.guard();
            let second = change_cognition_json_scalar_v1(
                CognitionJsonScalarChangeV1 {
                    field: CognitionJsonFieldV1::FeedbackOutbox,
                    qualification: &first,
                    path: &[JsonPathSegmentV1::Name("")],
                    change: (JsonScalarRefV1::Unsigned(9), JsonScalarRefV1::Text("同Txn")),
                },
                &mut overlay,
                &mut write,
                &mut owner_work,
                &guard,
                &live,
            )
            .unwrap()
            .unwrap();
            let guard = overlay.transaction.guard();
            assert!(
                change_cognition_json_scalar_v1(
                    CognitionJsonScalarChangeV1 {
                        field: CognitionJsonFieldV1::FeedbackOutbox,
                        qualification: &second,
                        path: &[JsonPathSegmentV1::Name("")],
                        change: (
                            JsonScalarRefV1::Text("同Txn"),
                            JsonScalarRefV1::Text("同Txn")
                        )
                    },
                    &mut overlay,
                    &mut write,
                    &mut owner_work,
                    &guard,
                    &live
                )
                .unwrap()
                .is_none()
            );
            let guard = tx.guard();
            let sealed = tx.seal(&guard, &live, 1).unwrap();
            assert_eq!(sealed.record.writes.len(), 1);
            let verifier = TransitionVerifier {
                parent: &execution_parent,
                operation: [6; 32],
                source: &state,
                memory: &memory,
                class: MemoryClass::Ordinary,
                max_pending_node_bytes: 2 << 20,
            };
            let accepted = accept_cognition_json_entry_transition_v1(
                CognitionJsonFieldV1::FeedbackOutbox,
                &entry,
                &verifier,
                &sealed.record,
                &mut read,
                &mut JsonScalarChangeWorkV1 {
                    json: JsonReadWorkV1 {
                        cells_remaining: 64,
                        json_nodes_remaining: 8,
                        max_depth: 4,
                        max_payload_bytes: 4096,
                    },
                    paths_remaining: 4096,
                    max_pending_pairs: 8,
                },
            )
            .unwrap();
            assert_eq!(accepted.body_root(), second.body_root());
            if let Some(parent_family) = &parent_family {
                let family = oasis7_runtime::incremental_cognition_family::accept_cognition_current_scalar_transition_v1(
                parent_family, &verifier, &sealed.record, &mut read,
                &mut JsonScalarChangeWorkV1 {
                    json:JsonReadWorkV1 {cells_remaining:64,json_nodes_remaining:8,max_depth:4,max_payload_bytes:4096},
                    paths_remaining:4096,max_pending_pairs:8,
                }).unwrap();
                let id = sealed.record.after.digest().unwrap();
                family.validate_binding(&id).unwrap();
                assert_eq!(
                    family
                        .json_entry(CognitionJsonFieldV1::FeedbackOutbox, &id)
                        .unwrap()
                        .unwrap()
                        .body_root(),
                    second.body_root()
                );
                assert!(
                    family
                        .validate_binding(&sealed.record.before.digest().unwrap())
                        .is_err()
                );
            }
            expected_feedback = accepted.body_root().canonical_bytes().unwrap();
            let generic = verifier.verify(&sealed.record).unwrap();
            assert!(generic.new_nodes.len() <= 257);
            let phase = memory.reserve(640 << 10, MemoryClass::Ordinary).unwrap();
            let delta = phase.retain(StateDeltaV1 {
                before: root.clone(),
                after: sealed
                    .record
                    .after
                    .state
                    .iter()
                    .find(|r| r.namespace == StateNamespaceV1::Effects)
                    .unwrap()
                    .clone(),
                writes: vec![sealed.record.writes[0].mutation.clone()],
                new_nodes: generic.new_nodes.clone(),
            });
            writer
                .put_state_pages_cancelled(&delta, &memory, &mut || false)
                .unwrap();
            roots[2].copy_from_slice(&delta.after.canonical_bytes().unwrap());
            // Independent query determines N. N+1 is the core's post-equality
            // check; N+2 closes the public overlay after its core returns.
            let mut cancelled_boundaries = [false; 4];
            for (boundary, (offset, channel)) in
                [(1, 0), (1, 1), (2, 0), (2, 1)].into_iter().enumerate()
            {
                use std::{cell::Cell, rc::Rc};
                let _inputs = memory.reserve(16 << 10, MemoryClass::Ordinary).unwrap();
                let prefix = "λ".repeat(1536);
                let same = "λ".repeat(1536);
                let failed_parent = ExecutionParent {
                    roots: sealed.record.after.clone(),
                    commit: [7; 32],
                    authority: execution_parent.authority,
                };
                let failed_live = failed_parent.fence().unwrap();
                let mut failed_tx = ExecutionTransaction::begin(
                    failed_parent,
                    [8; 32],
                    &state,
                    &memory,
                    TransactionLimits {
                        max_reads: 8,
                        max_writes: 2,
                        max_facts: 0,
                        max_pending_node_bytes: 2 << 20,
                    },
                )
                .unwrap();
                let read_count = Rc::new(Cell::new(0usize));
                let write_count = Rc::new(Cell::new(0usize));
                let read_limit = Rc::new(Cell::new(usize::MAX));
                let write_limit = Rc::new(Cell::new(usize::MAX));
                let poll =
                    |count: Rc<Cell<usize>>, limit: Rc<Cell<usize>>| -> Box<dyn FnMut() -> bool> {
                        Box::new(move || {
                            let next = count.get() + 1;
                            count.set(next);
                            next == limit.get()
                        })
                    };
                let mut input_io = fixture::logical();
                let mut input_keys = SemanticKeyBudget {
                    bytes_remaining: 16 << 20,
                };
                let mut failure_overlay = WorldFieldOverlayContextV1 {
                    transaction: &mut failed_tx,
                    values: ValueReadContext {
                        source: &reader,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut input_io,
                    },
                    key_work: &mut input_keys,
                    semantic_bytes: 4096,
                    record_bytes: 8192,
                    take_state_error: || state.take_read_error().map(state_error),
                    cancelled: poll(read_count.clone(), read_limit.clone()),
                };
                let mut output_io = fixture::logical();
                let mut output_keys = SemanticKeyBudget {
                    bytes_remaining: 16 << 20,
                };
                let mut failure_write = MemberRecordWriteContext {
                    sink: &mut writer,
                    memory: &memory,
                    io: &mut output_io,
                    key_work: &mut output_keys,
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 4096,
                        payload_bytes: 4096,
                        record_bytes: 8192,
                    },
                    cancelled: poll(write_count.clone(), write_limit.clone()),
                };
                let mut failure_work = CognitionJsonPointWorkV1 {
                    point: JsonWriteWorkV1 {
                        cells_remaining: 64,
                        json_nodes_remaining: 16,
                        max_json_depth: 4,
                        max_pending_node_bytes: 2 << 20,
                    },
                    changed: JsonScalarChangeWorkV1 {
                        json: JsonReadWorkV1 {
                            cells_remaining: 64,
                            json_nodes_remaining: 8,
                            max_depth: 4,
                            max_payload_bytes: 4096,
                        },
                        paths_remaining: 4096,
                        max_pending_pairs: 8,
                    },
                };
                let guard = failure_overlay.transaction.guard();
                let prefix_proof = change_cognition_json_scalar_v1(
                    CognitionJsonScalarChangeV1 {
                        field: CognitionJsonFieldV1::FeedbackOutbox,
                        qualification: &accepted,
                        path: &[JsonPathSegmentV1::Name("")],
                        change: (
                            JsonScalarRefV1::Text("同Txn"),
                            JsonScalarRefV1::Text(&prefix),
                        ),
                    },
                    &mut failure_overlay,
                    &mut failure_write,
                    &mut failure_work,
                    &guard,
                    &failed_live,
                )
                .unwrap()
                .unwrap();
                read_count.set(0);
                write_count.set(0);
                check_cognition_json_scalar_overlay_v1(
                    CognitionJsonScalarReadV1 {
                        field: CognitionJsonFieldV1::FeedbackOutbox,
                        qualification: &prefix_proof,
                        path: &[JsonPathSegmentV1::Name("")],
                        expected: JsonScalarRefV1::Text(&prefix),
                    },
                    &mut failure_overlay,
                    &mut JsonScalarPathReadWorkV1 {
                        cells_remaining: 16,
                        json_nodes_remaining: 8,
                        max_depth: 4,
                    },
                )
                .unwrap();
                let n = read_count.get();
                assert!(n > 0);
                assert_eq!(write_count.get(), 0);
                read_count.set(0);
                write_count.set(0);
                if channel == 0 {
                    read_limit.set(n + offset);
                } else {
                    write_limit.set(n + offset);
                }
                let guard = failure_overlay.transaction.guard();
                let result = change_cognition_json_scalar_v1(
                    CognitionJsonScalarChangeV1 {
                        field: CognitionJsonFieldV1::FeedbackOutbox,
                        qualification: &prefix_proof,
                        path: &[JsonPathSegmentV1::Name("")],
                        change: (JsonScalarRefV1::Text(&prefix), JsonScalarRefV1::Text(&same)),
                    },
                    &mut failure_overlay,
                    &mut failure_write,
                    &mut failure_work,
                    &guard,
                    &failed_live,
                );
                let was_cancelled = matches!(
                    result,
                    Err(
                        oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1::Member(
                            MemberRecordError::Value(ValueTreeError::Cancelled)
                        )
                    ) | Err(
                        oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(
                            ValueTreeError::Cancelled
                        )
                    )
                );
                let polls = if channel == 0 {
                    read_count.get()
                } else {
                    write_count.get()
                };
                let guard = failed_tx.guard();
                let sealed_prefix = failed_tx.seal(&guard, &failed_live, 2);
                let aborted = matches!(sealed_prefix, Err(ExecutionError::Aborted));
                cancelled_boundaries[boundary] = was_cancelled && polls == n + offset && aborted;
                eprintln!(
                    "Cognition independent getter N={n} offset={offset} channel={channel} polls={polls} cancelled={was_cancelled} sealable={} result={result:?}",
                    sealed_prefix.is_ok()
                );
            }
            assert!(
                cancelled_boundaries.into_iter().all(|passed| passed),
                "core/overlay NoOp boundaries={cancelled_boundaries:?}"
            );
        }
        eprintln!(
            "Cognition final writer scope all_optional={all_optional} remaining={:?}",
            writer.remaining_io().unwrap()
        );
    }
    let cold = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let store = IncrementalPageStore::open(
        &path,
        cold.clone(),
        PageCompression::Raw,
        MemoryClass::Recovery,
    )
    .unwrap();
    let cold_physical = PageIoBudget {
        reads_remaining: if all_optional { 3_000_000 } else { 2_500_000 },
        ..physical()
    };
    let state = store
        .state_session(cold_physical.clone(), MemoryClass::Recovery)
        .unwrap();
    let values = store.value_session(cold_physical);
    let mut io = ValueIoBudget {
        nodes_remaining: if all_optional { 200_000 } else { 100_000 },
        ..fixture::logical()
    };
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut work = oasis7_runtime::incremental_json_records::JsonReadWorkV1 {
        cells_remaining: 4096,
        json_nodes_remaining: 512,
        max_depth: 4,
        max_payload_bytes: 4096,
    };
    for field in CognitionJsonFieldV1::ALL {
        let namespace = match field.descriptor().destination {
            FieldDestination::State(ns) => ns,
            _ => unreachable!(),
        };
        let root = StateRootV1::from_canonical_bytes(
            &roots[namespaces.iter().position(|ns| *ns == namespace).unwrap()],
        )
        .unwrap();
        let mut context = WorldFieldReadContextV1 {
            root: &root,
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &cold,
                class: MemoryClass::Recovery,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let result = qualify_cognition_json_entry_v1(field, &mut context, &mut work);
        eprintln!(
            "Cognition cold field={field:?} result_ok={} logical={:?} state={:?} values={:?} cells={} json_nodes={}",
            result.is_ok(),
            context.values.io,
            state.remaining_io().unwrap(),
            values.remaining_io().unwrap(),
            work.cells_remaining,
            work.json_nodes_remaining
        );
        let Some(proof) = result.unwrap() else {
            assert!(field.optional());
            continue;
        };
        if field == CognitionJsonFieldV1::FeedbackOutbox {
            assert_eq!(
                proof.body_root().canonical_bytes().unwrap(),
                expected_feedback
            );
        }

        proof.validate_binding(field, proof.body_root()).unwrap();
        if !all_optional
            || matches!(
                field,
                CognitionJsonFieldV1::FeedbackOutbox
                    | CognitionJsonFieldV1::IdempotencyIndex
                    | CognitionJsonFieldV1::StagedActions
                    | CognitionJsonFieldV1::SchedulerState
                    | CognitionJsonFieldV1::Continuations
                    | CognitionJsonFieldV1::ContinuationContexts
            )
        {
            assert_eq!(
                proof.body_root().leaf_count,
                if field == CognitionJsonFieldV1::FeedbackOutbox {
                    6
                } else if field == CognitionJsonFieldV1::Continuations {
                    7
                } else {
                    5
                }
            );
        }
        assert!(
            proof
                .validate_binding(CognitionJsonFieldV1::ContinuationEvaluations, &root)
                .is_err()
        );
    }
    for field in CognitionControlFieldV1::ALL {
        let namespace = match field.descriptor().destination {
            FieldDestination::State(ns) => ns,
            _ => unreachable!(),
        };
        let root = StateRootV1::from_canonical_bytes(
            &roots[namespaces.iter().position(|ns| *ns == namespace).unwrap()],
        )
        .unwrap();
        let mut context = WorldFieldReadContextV1 {
            root: &root,
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &cold,
                class: MemoryClass::Recovery,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let _out = cold.reserve(1024, MemoryClass::Recovery).unwrap();
        let mut output = Vec::with_capacity(1024);
        let actual = read_cognition_control_v1(field, &mut context, &mut output).unwrap();
        if actual == CognitionControlReadV1::Absent {
            assert!(field.optional() && !all_optional);
            continue;
        }
        match field {
            CognitionControlFieldV1::JournalHeadSeq => {
                assert_eq!(actual, CognitionControlReadV1::Unsigned(u64::MAX))
            }
            CognitionControlFieldV1::ReorgInvalidationEpoch => {
                assert_eq!(actual, CognitionControlReadV1::Unsigned(0))
            }
            CognitionControlFieldV1::Schema => assert_eq!(output, b"cognition-persistence.v1"),
            CognitionControlFieldV1::JournalSchema => assert_eq!(output, b"cognition-journal.v1"),
            CognitionControlFieldV1::JournalHeadDigest => assert_eq!(output, "原digest".as_bytes()),
            CognitionControlFieldV1::EconomySchema => assert_eq!(
                output,
                oasis7_runtime::COGNITION_ECONOMY_SCHEMA_VERSION.as_bytes()
            ),
            CognitionControlFieldV1::EconomyProvisionHeadSeq => {
                assert_eq!(actual, CognitionControlReadV1::Unsigned(42))
            }
            CognitionControlFieldV1::EconomyHeadSeq => {
                assert_eq!(actual, CognitionControlReadV1::Unsigned(u64::MAX))
            }
            CognitionControlFieldV1::EconomyHeadDigest => {
                assert_eq!(output, "原economic-digest".as_bytes())
            }
            CognitionControlFieldV1::EconomyProvisionHeadDigest => assert!(output.is_empty()),
        }
    }
    {
        use oasis7_formal_protocol::incremental_execution::RootSetV1;
        use oasis7_runtime::incremental_cognition_family::*;
        let _root_metadata = cold.reserve(32 << 10, MemoryClass::Recovery).unwrap();
        let mut root_set = RootSetV1::empty("cognition-test".into(), [0x52; 32]).unwrap();
        for (index, namespace) in namespaces.iter().enumerate() {
            *root_set
                .state
                .iter_mut()
                .find(|root| root.namespace == *namespace)
                .unwrap() = StateRootV1::from_canonical_bytes(&roots[index]).unwrap();
        }
        let scope = PageIoBudget {
            reads_remaining: if all_optional { 3_000_000 } else { 2_500_000 },
            ..physical()
        };
        let family_state = store
            .state_session(scope.clone(), MemoryClass::Recovery)
            .unwrap();
        let family_values = store.value_session(scope);
        let mut family_io = ValueIoBudget {
            nodes_remaining: if all_optional { 200_000 } else { 100_000 },
            ..fixture::logical()
        };
        let mut family_keys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut context = WorldFieldReadContextV1 {
            root: &root_set.state[0],
            state: &family_state,
            values: ValueReadContext {
                source: &family_values,
                memory: &cold,
                class: MemoryClass::Recovery,
                io: &mut family_io,
            },
            key_work: &mut family_keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || family_state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let family = qualify_cognition_current_state_v1(
            &root_set,
            &mut context,
            &mut StateScanBudget {
                nodes_remaining: 100_000,
                read_bytes_remaining: 64 << 20,
                leaves_remaining: 1000,
            },
            &mut oasis7_runtime::incremental_json_records::JsonReadWorkV1 {
                cells_remaining: 4096,
                json_nodes_remaining: 512,
                max_depth: 4,
                max_payload_bytes: 4096,
            },
            &mut 1000,
        )
        .unwrap();
        let id = root_set.digest().unwrap();
        family.validate_binding(&id).unwrap();
        assert!(family.validate_binding(&[0; 32]).is_err());
        assert_eq!(
            family.control_present(CognitionControlFieldV1::EconomySchema),
            all_optional
        );
        for field in CognitionJsonFieldV1::ALL {
            let proof = family.json_entry(field, &id).unwrap();
            assert_eq!(
                proof.is_some(),
                all_optional
                    || !field.optional()
                    || field == CognitionJsonFieldV1::ContinuationContexts
            );
            if field == CognitionJsonFieldV1::FeedbackOutbox {
                assert_eq!(
                    proof.unwrap().body_root().canonical_bytes().unwrap(),
                    expected_feedback
                );
            }
        }
        if !all_optional {
            family_faults::reject_malformed_outer(&store, &cold, &root_set);
            cognition_pages::transfer_and_reopen(
                &path,
                &directory.0.join("cognition-copy"),
                &root_set,
            );
        }
        eprintln!(
            "Cognition full current-state family Some={all_optional} physicalState={:?} values={:?} logical={:?}",
            family_state.remaining_io().unwrap(),
            family_values.remaining_io().unwrap(),
            context.values.io
        );
    }

    eprintln!(
        "Cognition entry cold physical state={:?} value={:?} logical={:?}",
        state.remaining_io().unwrap(),
        values.remaining_io().unwrap(),
        io
    );
}
