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

fn tx_step<
    'a,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter + ValueNodeReader,
    CW: FnMut() -> bool,
>(
    parent: &'a ReservedValue<CognitionJsonEntryQualificationV1>,
    path: &'a [JsonPathSegmentV1<'a>],
    edit: JsonSubtreeEditV1<'a>,
    context: &mut oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1<
        '_,
        '_,
        '_,
        M,
        R,
        E,
        C,
    >,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut oasis7_runtime::incremental_cognition_json_overlay::CognitionJsonPointWorkV1,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
) -> Option<ReservedValue<CognitionJsonEntryQualificationV1>> {
    use oasis7_runtime::incremental_cognition_json_overlay::*;
    let guard = context.transaction.guard();
    change_cognition_json_subtree_v1(
        CognitionJsonSubtreeChangeV1 {
            field: CognitionJsonFieldV1::FeedbackOutbox,
            qualification: parent,
            path,
            edit,
        },
        context,
        write,
        work,
        &guard,
        live,
    )
    .unwrap()
}
pub fn same_transaction(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    parent: &oasis7_formal_protocol::incremental_execution::ExecutionParent,
    family: &ReservedValue<
        oasis7_runtime::incremental_cognition_family::CognitionCurrentStateQualificationV1,
    >,
) {
    use oasis7_formal_protocol::incremental_execution::*;
    use oasis7_runtime::{
        incremental_cognition_family::*, incremental_cognition_json_overlay::*,
        incremental_field_overlay::WorldFieldOverlayContextV1,
    };
    let _fixture = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
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
    let mut tx = ExecutionTransaction::begin(
        parent.clone(),
        [0x63; 32],
        &state,
        memory,
        TransactionLimits {
            max_reads: 8,
            max_writes: 2,
            max_facts: 0,
            max_pending_node_bytes: 2 << 20,
        },
    )
    .unwrap();
    let live = parent.fence().unwrap();
    let mut io = ValueIoBudget {
        nodes_remaining: 8 * 100_000,
        read_bytes_remaining: 8 * (256 << 20),
        ..fixture::logical()
    };
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut write_io = fixture::logical();
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let id = parent.roots.digest().unwrap();
    let initial = family
        .json_entry(CognitionJsonFieldV1::FeedbackOutbox, &id)
        .unwrap()
        .unwrap();
    let last;
    {
        let mut context = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
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
        let mut work = CognitionJsonPointWorkV1 {
            point: JsonWriteWorkV1 {
                cells_remaining: 4096,
                json_nodes_remaining: 512,
                max_json_depth: 8,
                max_pending_node_bytes: 2 << 20,
            },
            changed: JsonChangeWorkV1 {
                json: JsonReadWorkV1 {
                    cells_remaining: 16_384,
                    json_nodes_remaining: 1024,
                    max_depth: 8,
                    max_payload_bytes: 4096,
                },
                paths_remaining: 65_536,
                max_pending_pairs: 32,
            },
        };
        let empty = serde_json::json!({});
        let mut current = tx_step(
            initial,
            &[],
            JsonSubtreeEditV1::ObjectMember {
                name: "new",
                expected: None,
                after: Some(&empty),
            },
            &mut context,
            &mut write,
            &mut work,
            &live,
        )
        .unwrap();
        let array = serde_json::json!([true, true, null]);
        current = tx_step(
            &current,
            &[JsonPathSegmentV1::Name("new")],
            JsonSubtreeEditV1::ObjectMember {
                name: "dupe",
                expected: None,
                after: Some(&array),
            },
            &mut context,
            &mut write,
            &mut work,
            &live,
        )
        .unwrap();
        let path = [
            JsonPathSegmentV1::Name("new"),
            JsonPathSegmentV1::Name("dupe"),
        ];
        let appended = [serde_json::json!(u64::MAX), serde_json::json!({})];
        current = tx_step(
            &current,
            &path,
            JsonSubtreeEditV1::Append {
                expected_len: 3,
                values: &appended,
            },
            &mut context,
            &mut write,
            &mut work,
            &live,
        )
        .unwrap();
        current = tx_step(
            &current,
            &path,
            JsonSubtreeEditV1::Truncate {
                expected_len: 5,
                len: 2,
            },
            &mut context,
            &mut write,
            &mut work,
            &live,
        )
        .unwrap();
        assert!(
            tx_step(
                &current,
                &path,
                JsonSubtreeEditV1::Append {
                    expected_len: 2,
                    values: &[]
                },
                &mut context,
                &mut write,
                &mut work,
                &live
            )
            .is_none()
        );
        let old = qualify_cognition_json_path_overlay_v1(
            CognitionJsonPathReadV1 {
                field: CognitionJsonFieldV1::FeedbackOutbox,
                qualification: &current,
                path: &path,
            },
            &mut context,
            &mut JsonScalarPathReadWorkV1 {
                cells_remaining: 32,
                json_nodes_remaining: 16,
                max_depth: 8,
            },
        )
        .unwrap();
        assert_eq!(old.root().leaf_count, 7);
        let text = serde_json::json!("留");
        current = tx_step(
            &current,
            &path,
            JsonSubtreeEditV1::Replace {
                expected: old.root(),
                after: &text,
            },
            &mut context,
            &mut write,
            &mut work,
            &live,
        )
        .unwrap();
        check_cognition_json_scalar_overlay_v1(
            CognitionJsonScalarReadV1 {
                field: CognitionJsonFieldV1::FeedbackOutbox,
                qualification: &current,
                path: &path,
                expected: JsonScalarRefV1::Text("留"),
            },
            &mut context,
            &mut JsonScalarPathReadWorkV1 {
                cells_remaining: 32,
                json_nodes_remaining: 16,
                max_depth: 8,
            },
        )
        .unwrap();
        last = current.body_root().canonical_bytes().unwrap();
    }
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 9).unwrap();
    assert_eq!(sealed.record.writes.len(), 1);
    let verifier = TransitionVerifier {
        parent,
        operation: [0x63; 32],
        source: &state,
        memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 2 << 20,
    };
    let root = parent
        .roots
        .state
        .iter()
        .find(|root| root.namespace == StateNamespaceV1::Effects)
        .unwrap();
    let mut context = WorldFieldReadContextV1 {
        root,
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
    let accepted = accept_cognition_current_transition_v1(
        family,
        &verifier,
        &sealed.record,
        &mut context,
        &mut JsonChangeWorkV1 {
            json: JsonReadWorkV1 {
                cells_remaining: 1024,
                json_nodes_remaining: 128,
                max_depth: 8,
                max_payload_bytes: 4096,
            },
            paths_remaining: 32_768,
            max_pending_pairs: 16,
        },
    )
    .unwrap();
    let id = sealed.record.after.digest().unwrap();
    accepted.validate_binding(&id).unwrap();
    assert_eq!(
        accepted
            .json_entry(CognitionJsonFieldV1::FeedbackOutbox, &id)
            .unwrap()
            .unwrap()
            .body_root()
            .canonical_bytes()
            .unwrap(),
        last
    );
    let generic = verifier.verify(&sealed.record).unwrap();
    let after = sealed
        .record
        .after
        .state
        .iter()
        .find(|root| root.namespace == StateNamespaceV1::Effects)
        .unwrap();
    let heap = memory.reserve(640 << 10, MemoryClass::Ordinary).unwrap();
    let delta = heap.retain(StateDeltaV1 {
        before: root.clone(),
        after: after.clone(),
        writes: vec![sealed.record.writes[0].mutation.clone()],
        new_nodes: generic.new_nodes.clone(),
    });
    writer
        .put_state_pages_cancelled(&delta, memory, &mut || false)
        .unwrap();
    let raw = super::family_faults::qualify_ordinary(store, memory, &sealed.record.after);
    assert_eq!(
        raw.json_entry(CognitionJsonFieldV1::FeedbackOutbox, &id)
            .unwrap()
            .unwrap()
            .body_root()
            .canonical_bytes()
            .unwrap(),
        last
    );
    eprintln!(
        "Cognition actual shared structural transaction accepted and raw verified with one outer write"
    );
}
