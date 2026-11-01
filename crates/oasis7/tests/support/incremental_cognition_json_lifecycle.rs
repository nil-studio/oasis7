//! Explicit full JSON oracle scopes are separate from normal point metrics.
use super::*;
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_runtime::incremental_json_records::*;
type Proof = ReservedValue<JsonSubtreeQualificationV1>;
struct Work {
    write: JsonWriteWorkV1,
    changed: JsonChangeWorkV1,
    oracle: JsonReadWorkV1,
}
fn step<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
>(
    parent: &Proof,
    path: &[JsonPathSegmentV1<'_>],
    edit: JsonSubtreeEditV1<'_>,
    read: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut Work,
) -> (Proof, bool) {
    let profile = CognitionJsonFieldV1::FeedbackOutbox.profile();
    let prepared =
        prepare_json_subtree_change_v1(profile, parent, path, edit, read, write, &mut work.write)
            .unwrap();
    assert_eq!(prepared.before(), parent.root());
    let next = advance_json_subtree_v1(profile, parent, prepared.after(), read, &mut work.changed)
        .unwrap();
    let raw = qualify_json_subtree_v1(profile, prepared.after(), read, &mut work.oracle).unwrap();
    assert_eq!(next.root(), raw.root());
    assert!(next.node_count_upper_bound() >= raw.exact_node_count().unwrap());
    (next, prepared.changed())
}
pub fn exercise(store: &IncrementalPageStore, memory: &MemoryBudget, parent: &Proof) {
    let _fixture = memory.reserve(32 << 10, MemoryClass::Ordinary).unwrap();
    // Eight complete independent after-image oracles share one finite scope;
    // this is not a production point/cold allowance or a performance result.
    let scope = PageIoBudget {
        reads_remaining: 8 * 2_500_000,
        read_bytes_remaining: 8 * (256 << 20),
        ..physical()
    };
    let state = store
        .state_session(scope.clone(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(scope);
    let mut writer = store.value_session(physical());
    let mut read_io = ValueIoBudget {
        nodes_remaining: 8 * 100_000,
        read_bytes_remaining: 8 * (256 << 20),
        ..fixture::logical()
    };
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut read = WorldFieldReadContextV1 {
        root: parent.root(),
        state: &state,
        values: ValueReadContext {
            source: &values,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut read_io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 8192,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    let mut write_io = fixture::logical();
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
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
    let mut work = Work {
        write: JsonWriteWorkV1 {
            cells_remaining: 4096,
            json_nodes_remaining: 256,
            max_json_depth: 8,
            max_pending_node_bytes: 2 << 20,
        },
        changed: JsonChangeWorkV1 {
            json: JsonReadWorkV1 {
                cells_remaining: 4096,
                json_nodes_remaining: 256,
                max_depth: 8,
                max_payload_bytes: 4096,
            },
            paths_remaining: 32_768,
            max_pending_pairs: 16,
        },
        oracle: JsonReadWorkV1 {
            cells_remaining: 4096,
            json_nodes_remaining: 256,
            max_depth: 8,
            max_payload_bytes: 4096,
        },
    };
    let empty = serde_json::json!({});
    let (mut current, changed) = step(
        parent,
        &[],
        JsonSubtreeEditV1::ObjectMember {
            name: "new",
            expected: None,
            after: Some(&empty),
        },
        &mut read,
        &mut write,
        &mut work,
    );
    assert!(changed);
    assert_eq!(current.exact_node_count(), None);
    let array = serde_json::json!([true, true, null]);
    (current, _) = step(
        &current,
        &[JsonPathSegmentV1::Name("new")],
        JsonSubtreeEditV1::ObjectMember {
            name: "dupe",
            expected: None,
            after: Some(&array),
        },
        &mut read,
        &mut write,
        &mut work,
    );
    let path = [
        JsonPathSegmentV1::Name("new"),
        JsonPathSegmentV1::Name("dupe"),
    ];
    let appended = [serde_json::json!(u64::MAX), serde_json::json!({})];
    (current, _) = step(
        &current,
        &path,
        JsonSubtreeEditV1::Append {
            expected_len: 3,
            values: &appended,
        },
        &mut read,
        &mut write,
        &mut work,
    );
    (current, _) = step(
        &current,
        &path,
        JsonSubtreeEditV1::Truncate {
            expected_len: 5,
            len: 2,
        },
        &mut read,
        &mut write,
        &mut work,
    );
    for index in 0..2 {
        check_json_scalar_path_expected_v1(
            CognitionJsonFieldV1::FeedbackOutbox.profile(),
            &current,
            &[
                JsonPathSegmentV1::Name("new"),
                JsonPathSegmentV1::Name("dupe"),
                JsonPathSegmentV1::Index(index),
            ],
            JsonScalarRefV1::Bool(true),
            &mut read,
            &mut JsonScalarPathReadWorkV1 {
                cells_remaining: 32,
                json_nodes_remaining: 16,
                max_depth: 8,
            },
        )
        .unwrap();
    }
    let (next, changed) = step(
        &current,
        &path,
        JsonSubtreeEditV1::Append {
            expected_len: 2,
            values: &[],
        },
        &mut read,
        &mut write,
        &mut work,
    );
    assert!(!changed);
    assert_eq!(next.exact_node_count(), None);
    current = next;
    let old = qualify_json_path_v1(
        CognitionJsonFieldV1::FeedbackOutbox.profile(),
        &current,
        &path,
        &mut read,
        &mut JsonScalarPathReadWorkV1 {
            cells_remaining: 32,
            json_nodes_remaining: 16,
            max_depth: 8,
        },
    )
    .unwrap();
    let text = serde_json::json!("留");
    (current, _) = step(
        &current,
        &path,
        JsonSubtreeEditV1::Replace {
            expected: old.root(),
            after: &text,
        },
        &mut read,
        &mut write,
        &mut work,
    );
    let member = qualify_json_path_v1(
        CognitionJsonFieldV1::FeedbackOutbox.profile(),
        &current,
        &[JsonPathSegmentV1::Name("new")],
        &mut read,
        &mut JsonScalarPathReadWorkV1 {
            cells_remaining: 32,
            json_nodes_remaining: 16,
            max_depth: 8,
        },
    )
    .unwrap();
    (current, _) = step(
        &current,
        &[],
        JsonSubtreeEditV1::ObjectMember {
            name: "new",
            expected: Some(member.root()),
            after: None,
        },
        &mut read,
        &mut write,
        &mut work,
    );
    assert_eq!(current.root(), parent.root());
    let (current, changed) = step(
        &current,
        &[],
        JsonSubtreeEditV1::ObjectMember {
            name: "new",
            expected: None,
            after: None,
        },
        &mut read,
        &mut write,
        &mut work,
    );
    assert!(!changed);
    assert_eq!(current.exact_node_count(), None);
    check_json_scalar_path_expected_v1(
        CognitionJsonFieldV1::FeedbackOutbox.profile(),
        &current,
        &[JsonPathSegmentV1::Name("")],
        JsonScalarRefV1::Bool(true),
        &mut read,
        &mut JsonScalarPathReadWorkV1 {
            cells_remaining: 16,
            json_nodes_remaining: 8,
            max_depth: 4,
        },
    )
    .unwrap();
    eprintln!(
        "Cognition JSON structural 8 complete scopes remaining state={:?} values={:?} writer={:?}",
        state.remaining_io().unwrap(),
        values.remaining_io().unwrap(),
        writer.remaining_io().unwrap()
    );
}
