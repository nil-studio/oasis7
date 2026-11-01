//! A typed member must use the complete original canonical field-key tuple.
use super::*;
use oasis7_runtime::{incremental_json_records::*, incremental_semantic_key::hash_semantic_key_v1};
pub fn reject_relabelled_member(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    original: &StateRootV1,
) {
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let mut writer = store.value_session(physical());
    let values = store.value_session(physical());
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let semantic = hash_semantic_key_v1(
        &(((), "members"), ""),
        memory,
        MemoryClass::Ordinary,
        &mut keys,
        &mut || false,
    )
    .unwrap();
    let profile = CognitionJsonFieldV1::FeedbackOutbox.profile();
    let key = WorldFieldKeyV1::new(
        profile.field,
        WorldFieldRecordKindV1::Entry,
        Some(semantic.digest),
    )
    .unwrap();
    let value = original.get(key.as_bytes(), &state).unwrap().unwrap();
    let other = FieldDescriptor {
        destination: FieldDestination::State(StateNamespaceV1::Authorization),
        ..profile.field
    };
    let wrong =
        WorldFieldKeyV1::new(other, WorldFieldRecordKindV1::Entry, Some(semantic.digest)).unwrap();
    assert_ne!(key.as_bytes(), wrong.as_bytes());
    let charged = memory
        .reserve(2 * (512 << 10) + (128 << 10), MemoryClass::Ordinary)
        .unwrap();
    let mut writes = vec![
        StateWriteV1 {
            key: key.as_bytes().to_vec(),
            before: Some(value),
            after: None,
        },
        StateWriteV1 {
            key: wrong.as_bytes().to_vec(),
            before: None,
            after: Some(value),
        },
    ];
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let delta =
        charged.retain(StateDeltaV1::build(original.clone(), writes, &state, 2 << 20).unwrap());
    writer
        .put_state_pages_cancelled(&delta, memory, &mut || false)
        .unwrap();
    assert_eq!(delta.after.leaf_count, original.leaf_count);
    assert_eq!(delta.after.get(key.as_bytes(), &state).unwrap(), None);
    assert_eq!(
        delta.after.get(wrong.as_bytes(), &state).unwrap(),
        Some(value)
    );
    let mut io = fixture::logical();
    let mut context = WorldFieldReadContextV1 {
        root: original,
        state: &state,
        values: ValueReadContext {
            source: &values,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 8192,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    let result = qualify_json_subtree_v1(
        profile,
        &delta.after,
        &mut context,
        &mut JsonReadWorkV1 {
            cells_remaining: 128,
            json_nodes_remaining: 24,
            max_depth: 4,
            max_payload_bytes: 4096,
        },
    );
    assert!(
        matches!(result, Err(ValueTreeError::Invalid(_))),
        "wrong encoded member namespace accepted: {result:?}"
    );
}
