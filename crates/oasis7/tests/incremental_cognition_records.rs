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
fn source() -> serde_json::Value {
    serde_json::json!({"schema_version":"cognition-persistence.v1", "cognition_journal":{"schema_version":"cognition-journal.v1","head_seq":u64::MAX,"head_digest":"原digest","events":[]}, "responses":[],"commit_records":[],"receipt_registry":[],"receipt_lineage_registry":[],"feedback_outbox":{"":true},"idempotency_index":{},"staged_actions":{},"scheduler_state":{},"continuations":[null,""],"recovery":null,"continuation_contexts":{},"reorg_invalidation_epoch":0})
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
    let directory = fixture::Directory::new();
    let memory = MemoryBudget::new(32 << 20, 8 << 20).unwrap();
    let path = directory.0.join("cognition");
    let store = IncrementalPageStore::create(&path, memory.clone(), PageCompression::Raw).unwrap();
    let _input = memory.reserve(16 << 10, MemoryClass::Ordinary).unwrap();
    let source = source();
    let _batch = memory.reserve(32 << 10, MemoryClass::Ordinary).unwrap();
    let mut writes = Vec::with_capacity(24);
    let mut writer = store.value_session(physical());
    let mut io = fixture::logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut records = 24;
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
            assert!(writes.len() < 24);
            writes.push(write);
            Ok(())
        },
    )
    .unwrap();
    let mut json_work = oasis7_runtime::incremental_json_records::JsonWriteWorkV1 {
        cells_remaining: 128,
        json_nodes_remaining: 24,
        max_json_depth: 4,
        max_pending_node_bytes: 2 << 20,
    };
    prepare_cognition_json_genesis_v1(
        CognitionEnvelopeRefV1::new(&source).unwrap(),
        &mut ctx,
        &mut records,
        &mut json_work,
        &mut |write| {
            assert!(writes.len() < 24);
            writes.push(write);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(writes.len(), 22);
    assert_eq!(records, 2);
    let namespaces = [
        StateNamespaceV1::Artifacts,
        StateNamespaceV1::Allocator,
        StateNamespaceV1::Effects,
        StateNamespaceV1::Authorization,
        StateNamespaceV1::Scheduler,
    ];
    let mut roots = [[0u8; STATE_ROOT_BYTES]; 5];
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
        let after = qualify_json_subtree_v1(
            CognitionJsonFieldV1::FeedbackOutbox.profile(),
            changed.after(),
            &mut read,
            &mut full,
        )
        .unwrap();
        let noop = prepare_json_scalar_path_change_v1(
            CognitionJsonFieldV1::FeedbackOutbox.profile(),
            &after,
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
    }
    drop(writes);
    drop(writer);
    drop(state);
    drop(store);
    drop(source);
    drop(_input);
    drop(_batch);
    drop(memory);
    let cold = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let store = IncrementalPageStore::open(
        &path,
        cold.clone(),
        PageCompression::Raw,
        MemoryClass::Recovery,
    )
    .unwrap();
    let state = store
        .state_session(physical(), MemoryClass::Recovery)
        .unwrap();
    let values = store.value_session(physical());
    let mut io = fixture::logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut work = oasis7_runtime::incremental_json_records::JsonReadWorkV1 {
        cells_remaining: 128,
        json_nodes_remaining: 24,
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
        let proof = qualify_cognition_json_entry_v1(field, &mut context, &mut work).unwrap();
        if field == CognitionJsonFieldV1::ContinuationEvaluations {
            assert!(proof.is_none());
        } else {
            let proof = proof.unwrap();
            proof.validate_binding(field, &root).unwrap();
            assert_eq!(
                proof.body_root().leaf_count,
                if matches!(field, CognitionJsonFieldV1::FeedbackOutbox) {
                    6
                } else if field == CognitionJsonFieldV1::Continuations {
                    7
                } else {
                    5
                }
            );
            assert!(
                proof
                    .validate_binding(CognitionJsonFieldV1::ContinuationEvaluations, &root)
                    .is_err()
            );
        }
    }
    for field in CognitionControlFieldV1::REQUIRED
        .into_iter()
        .chain([CognitionControlFieldV1::ReorgInvalidationEpoch])
    {
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
        }
    }
    eprintln!(
        "Cognition entry cold physical state={:?} value={:?}",
        state.remaining_io().unwrap(),
        values.remaining_io().unwrap()
    );
}
