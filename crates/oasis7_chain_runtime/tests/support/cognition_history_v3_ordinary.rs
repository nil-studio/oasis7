//! Candidate receiver for the real two-event ordinary producer fixture.
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::ExecutionFactKindV1,
    incremental_material::{CandidateFactRequestV1, check_candidate_fact_v1},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::{
    incremental_cognition_family::CognitionCurrentStateQualificationV1,
    incremental_cognition_history_cbor::*,
    incremental_cognition_history_hash_outboard::CognitionHeadHashWorkV1,
    incremental_cognition_lifecycle_index_v2::CognitionLifecycleWorkV2,
    incremental_cognition_lifecycle_qualification_v2::*,
};
pub(super) struct OrdinaryFrame<'a> {
    pub before: &'a ReservedValue<AcceptedCognitionHistoryViewV1>,
    pub before_current: &'a ReservedValue<CognitionHistoryCurrentBindingV1>,
    pub before_journal: &'a ReservedValue<CognitionHistoryIndexQualificationV1>,
    pub before_lifecycle: &'a ReservedValue<CognitionLifecycleQualificationV2>,
    pub after_current: &'a ReservedValue<CognitionCurrentStateQualificationV1>,
    pub after_journal: &'a ReservedValue<CognitionHistoryIndexQualificationV1>,
}
fn hashes() -> CognitionHeadHashWorkV1 {
    CognitionHeadHashWorkV1 {
        input_bytes_remaining: 32 << 20,
        hash_nodes_remaining: 1_000_000,
    }
}
fn indexes() -> CognitionHistoryIndexWorkV1 {
    CognitionHistoryIndexWorkV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 4 << 20,
    }
}
fn cbor() -> CognitionHistoryCborWorkV1 {
    CognitionHistoryCborWorkV1 {
        bytes_remaining: 16 << 20,
        nodes_remaining: 100_000,
        source_reads_remaining: 100_000,
        max_depth: 64,
        key_bytes_remaining: 1 << 20,
        max_key_bytes: 4096,
    }
}
pub(super) fn check_ordinary(
    frame: OrdinaryFrame<'_>,
    request: &CognitionHistorySelectionRequestV1<'_>,
    store: &IncrementalPageStore,
    objects: &IncrementalObjectStore<'_>,
    io: &mut ObjectIo<'_>,
    memory: &MemoryBudget,
) -> ReservedValue<CheckedCognitionHistorySourceV3> {
    let selected = check_candidate_cognition_history_view_v1(
        request,
        &mut objects.original_reader(io),
        memory,
        &mut super::signed::facts(),
        || false,
    )
    .unwrap();
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut writer = store.value_session(physical());
    let mut read_io = logical();
    let mut write_io = logical();
    let mut read_keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut read = WorldFieldReadContextV1 {
        root: request
            .core
            .roots()
            .state_root(oasis7_formal_protocol::incremental_state::StateNamespaceV1::Allocator),
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
    let parent_journal = recover_attested_cognition_history_index_v3(
        CognitionHistoryFieldV1::JournalEvents,
        frame.before,
        frame.before_current,
        memory,
        || false,
    )
    .unwrap();
    let parent_response = recover_attested_cognition_history_index_v3(
        CognitionHistoryFieldV1::Responses,
        frame.before,
        frame.before_current,
        memory,
        || false,
    )
    .unwrap();
    // The first real ordinary transition starts from an explicitly empty GEN.
    // Later cold inheritance uses the v3 capsule rather than this bootstrap.
    assert_eq!(frame.before_journal.record_count(), 0);
    let parent_source =
        AuthenticatedHeadStreamV1::new(frame.before, frame.before_journal, memory, || false)
            .unwrap()
            .finish(frame.before, memory, &mut hashes(), || false)
            .unwrap();
    let mut head = CandidateCognitionHeadSourceBuilderV3::ordinary(
        request,
        &selected,
        frame.before,
        &parent_source,
        &parent_journal,
        memory,
        || false,
    )
    .unwrap();
    let mut journal = CandidateCognitionHistoryIndexBuilderV3::new(
        request,
        &selected,
        CognitionHistoryFieldV1::JournalEvents,
        None,
        Some((&parent_journal, frame.before)),
        memory,
        || false,
    )
    .unwrap();
    let response = CandidateCognitionHistoryIndexBuilderV3::new(
        request,
        &selected,
        CognitionHistoryFieldV1::Responses,
        None,
        Some((&parent_response, frame.before)),
        memory,
        || false,
    )
    .unwrap();
    let mut lifecycle = CandidateCognitionLifecycleBuilderV3::new(
        request,
        &selected,
        Some((
            CognitionLifecycleParentSourceV3::Replayed(frame.before_lifecycle),
            frame.before,
        )),
        memory,
        || false,
    )
    .unwrap();
    let mut index_work = indexes();
    let mut hash_work = hashes();
    let mut cbor_work = cbor();
    let mut life_work = CognitionLifecycleWorkV2 {
        key_bytes_remaining: 1 << 20,
        cells_remaining: 100_000,
        max_pending_node_bytes: 4 << 20,
    };
    for ordinal in 0..frame.after_journal.record_count() {
        let point = read_cognition_history_index_point_v1(
            frame.after_journal,
            ordinal,
            &mut read,
            &mut index_work,
        )
        .unwrap();
        let row = point.row();
        let occurrence = check_candidate_fact_v1(
            &CandidateFactRequestV1 {
                kind: ExecutionFactKindV1::Event,
                candidate: request.candidate,
                core: request.core,
                originals: request.originals,
                ordinal: row.location.ordinal,
            },
            memory,
            || false,
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
            &mut cbor_work,
            || false,
        )
        .unwrap();
        cursor.finish().unwrap();
        let original = bind_checked_cognition_indexed_original_v3(
            point,
            schema,
            &occurrence,
            &selected,
            memory,
            || false,
        )
        .unwrap();
        journal
            .push(&original, &mut read, &mut write, &mut index_work)
            .unwrap();
        lifecycle
            .push(
                &original,
                ["agent", "session", "turn", "decision", "request"],
                &mut read,
                &mut write,
                &mut life_work,
            )
            .unwrap();
        let point = read_cognition_history_index_point_v1(
            frame.after_journal,
            ordinal,
            &mut read,
            &mut index_work,
        )
        .unwrap();
        let mut cursor = objects
            .verified_original_cursor(&row.location.original, row.encoded_bytes, io)
            .unwrap();
        head.push_delta(
            point,
            &mut cursor,
            &mut read,
            &mut write,
            &mut index_work,
            &mut CandidateCognitionHeadSourceWorkV3 {
                cbor: &mut cbor_work,
                hashes: &mut hash_work,
            },
        )
        .unwrap();
        cursor.finish().unwrap();
    }
    let cache = recover_cognition_head_cache_v1(
        selected.derived().unwrap().caches[0].unwrap(),
        &mut read.values,
        &mut hash_work,
        || false,
    )
    .unwrap();
    let head = head.finish(&cache, &mut hash_work, || false).unwrap();
    let journal = journal.finish(|| false).unwrap();
    let response = response.finish(|| false).unwrap();
    let lifecycle = lifecycle.finish(|| false).unwrap();
    let current = bind_candidate_cognition_history_current_v3(
        request,
        &selected,
        request.core.roots(),
        CognitionHistoryCurrentSourceV1::Enabled(frame.after_current),
        &mut read,
    )
    .unwrap();
    qualify_checked_cognition_history_source_v3(
        CheckedCognitionHistoryComponentsV3 {
            selected,
            current,
            indexes: [Some(journal), Some(response), None, None],
            heads: [Some(head), None, None],
            lifecycle: Some(lifecycle),
            disabled: None,
        },
        memory,
        || false,
    )
    .unwrap()
}
