//! Actual production reader and prepaid promotion consumers; no proof mocks.
use super::signed::facts;
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1;
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::{
    incremental_cognition_history_cbor::CognitionHistoryCborWorkV1,
    incremental_cognition_history_hash_outboard::CognitionHeadHashWorkV1,
    incremental_cognition_lifecycle_index_v2::CognitionLifecycleWorkV2,
};
struct Originals<'a, 'b, 'c> {
    objects: &'a IncrementalObjectStore<'b>,
    io: &'a mut ObjectIo<'c>,
}
impl CognitionHistoryOriginalReadV3 for Originals<'_, '_, '_> {
    fn with_original<T>(
        &mut self,
        id: &[u8; 32],
        bytes: u64,
        consume: impl FnOnce(&mut dyn std::io::Read) -> Result<T, ValueTreeError>,
    ) -> Result<T, ValueTreeError> {
        let mut cursor = self
            .objects
            .verified_original_cursor(id, bytes, self.io)
            .map_err(state_error)?;
        if cursor.length() != bytes {
            return Err(ValueTreeError::Invalid("fixture original length"));
        }
        let result = consume(&mut cursor);
        cursor.finish().map_err(state_error)?;
        result
    }
}
pub(super) fn check(
    request: &CognitionHistorySelectionRequestV1<'_>,
    current: CognitionHistoryCurrentSourceV1<'_>,
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
    let mut sink = store.value_session(physical());
    let mut ri = logical();
    let mut wi = logical();
    let mut rk = SemanticKeyBudget {
        bytes_remaining: 32 << 20,
    };
    let mut wk = SemanticKeyBudget {
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
            io: &mut ri,
        },
        key_work: &mut rk,
        semantic_bytes: 4096,
        record_bytes: 8192,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut sink,
        memory,
        io: &mut wi,
        key_work: &mut wk,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 4096,
            record_bytes: 8192,
        },
        cancelled: || false,
    };
    let mut indexes = CognitionHistoryIndexWorkV1 {
        cells_remaining: 100000,
        max_pending_node_bytes: 8 << 20,
    };
    let mut cbor = CognitionHistoryCborWorkV1 {
        bytes_remaining: 32 << 20,
        nodes_remaining: 100000,
        source_reads_remaining: 100000,
        key_bytes_remaining: 1 << 20,
        max_depth: 64,
        max_key_bytes: 4096,
    };
    let mut hashes = CognitionHeadHashWorkV1 {
        input_bytes_remaining: 32 << 20,
        hash_nodes_remaining: 1000000,
    };
    let mut lifecycle = CognitionLifecycleWorkV2 {
        key_bytes_remaining: 1 << 20,
        cells_remaining: 100000,
        max_pending_node_bytes: 8 << 20,
    };
    check_candidate_complete_cognition_history_from_readers_v3(
        request,
        selected,
        current,
        parent,
        &mut read,
        &mut write,
        &mut Originals { objects, io },
        &mut CognitionHistoryCandidateReaderWorkV3 {
            indexes: &mut indexes,
            cbor: &mut cbor,
            hashes: &mut hashes,
            lifecycle: &mut lifecycle,
        },
    )
    .unwrap()
}
pub(super) fn prepare(
    checked: &ReservedValue<CheckedCognitionHistorySourceV3>,
    memory: &MemoryBudget,
) -> PreparedCompleteCognitionHistoryPromotionV3 {
    let before = memory.used();
    assert!(matches!(
        prepare_complete_cognition_history_promotion_v3(checked, memory, || true),
        Err(ValueTreeError::Cancelled)
    ));
    assert_eq!(memory.used(), before);
    let wrong = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    assert!(prepare_complete_cognition_history_promotion_v3(checked, &wrong, || false).is_err());
    assert_eq!(wrong.used(), 0);
    prepare_complete_cognition_history_promotion_v3(checked, memory, || false).unwrap()
}
/// Exhaust actual Ordinary admission only after every field was prepared.
/// This proves no new admission; it does not substitute for allocation tracing.
pub(super) fn finish_without_admission(
    prepared: PreparedCompleteCognitionHistoryPromotionV3,
    head: &VerifiedFormalCommitV1,
    memory: &MemoryBudget,
) -> ReservedValue<CompleteCognitionHistorySourceV3> {
    let error = match memory.reserve(usize::MAX, MemoryClass::Ordinary) {
        Err(e) => e,
        Ok(_) => panic!("bounded pool"),
    };
    let remaining = error.admission_limit.checked_sub(memory.used()).unwrap();
    let exhausted = memory.reserve(remaining, MemoryClass::Ordinary).unwrap();
    assert!(memory.reserve(1, MemoryClass::Ordinary).is_err());
    let result = finish_prepared_complete_cognition_history_promotion_v3(prepared, head).unwrap();
    drop(exhausted);
    result
}
