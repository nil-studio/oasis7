//! The actual default enabled projection: absent journal digest remains None.
use super::*;
use oasis7_formal_protocol::incremental_record_root::RecordRootRefV1;
use oasis7_world_state::incremental_cognition_history_hash_outboard::*;
use oasis7_world_state::incremental_cognition_lifecycle_qualification_v2::CandidateCognitionLifecycleBuilderV3;

fn hash_work() -> CognitionHeadHashWorkV1 {
    CognitionHeadHashWorkV1 {
        input_bytes_remaining: 128 << 20,
        hash_nodes_remaining: 1_000_000,
    }
}
struct HashWriter(blake3::Hasher);
impl std::io::Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(super) fn prepare(
    roots: &RootSetV1,
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
) -> (
    CognitionHistoryViewV1,
    CognitionHistoryDerivedRefsV2,
    Vec<u8>,
) {
    let mut pages = store.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut pages,
        memory,
        io: &mut io,
        key_work: &mut keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 8192,
            record_bytes: 8192,
        },
        cancelled: || false,
    };
    let indexes = std::array::from_fn(|i| {
        if i >= 2 {
            return CognitionHistoryIndexRefV1 {
                present: false,
                records: 0,
                root: StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap(),
            };
        }
        let field = if i == 0 {
            CognitionHistoryFieldV1::JournalEvents
        } else {
            CognitionHistoryFieldV1::Responses
        };
        CognitionHistoryIndexRefV1 {
            present: true,
            records: 0,
            root: prepare_cognition_history_index_v1(
                field,
                0,
                std::iter::empty::<Result<CognitionHistoryIndexRowV1, ValueTreeError>>(),
                &mut write,
                &mut CognitionHistoryIndexWorkV1 {
                    cells_remaining: 64,
                    max_pending_node_bytes: 1 << 20,
                },
            )
            .unwrap(),
        }
    });
    drop(write);
    let cache = CognitionHeadHashBuilderV1::new(memory, MemoryClass::Ordinary, || false)
        .unwrap()
        .finish(|| false)
        .unwrap();
    let cache_id = persist_cognition_head_cache_v1(
        &cache,
        &mut pages,
        memory,
        &mut io,
        &mut hash_work(),
        || false,
    )
    .unwrap();
    let mut hash = HashWriter(blake3::Hasher::new());
    cache
        .write_canonical_cache_v1(&mut hash, &mut hash_work(), || false)
        .unwrap();
    let view = CognitionHistoryViewV1 {
        roots: roots.digest().unwrap(),
        enabled: true,
        revision: 0,
        previous_view: None,
        heads: std::array::from_fn(|_| CognitionHistoryHeadV1 {
            sequence: 0,
            digest: None,
        }),
        indexes,
    };
    let refs = CognitionHistoryDerivedRefsV2 {
        caches: [Some(cache_id), None, None],
        lifecycle: Some(
            RecordRootRefV1::new(
                lifecycle_schema(),
                lifecycle_owner(),
                StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap(),
            )
            .unwrap(),
        ),
    };
    let summary = CognitionHistorySourceSummaryV3 {
        mode: CognitionHistorySourceModeV3::Bootstrap,
        cache_state_digests: [Some(*hash.0.finalize().as_bytes()), None, None],
        last: std::array::from_fn(|_| None),
        lifecycle_keys: 0,
        lifecycle_journal_records: 0,
    };
    let bytes = cognition_history_view_v3_bytes(&view, &refs, &summary, memory, || false).unwrap();
    (view, refs, bytes.to_vec())
}
pub(super) fn qualify<M, R, E, C>(
    request: &CognitionHistorySelectionRequestV1<'_>,
    selected: ReservedValue<CheckedCognitionHistoryViewV1>,
    view: &CognitionHistoryViewV1,
    refs: &CognitionHistoryDerivedRefsV2,
    current: &WorldCurrentStateQualificationV1,
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    memory: &MemoryBudget,
) -> ReservedValue<CheckedCognitionHistorySourceV3>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let journal = qualify_cognition_history_index_v1(
        CognitionHistoryFieldV1::JournalEvents,
        &view.indexes[0],
        ctx,
        &mut CognitionHistoryIndexWorkV1 {
            cells_remaining: 64,
            max_pending_node_bytes: 1 << 20,
        },
        |_, _| panic!("actual empty journal"),
    )
    .unwrap();
    let responses = qualify_cognition_history_index_v1(
        CognitionHistoryFieldV1::Responses,
        &view.indexes[1],
        ctx,
        &mut CognitionHistoryIndexWorkV1 {
            cells_remaining: 64,
            max_pending_node_bytes: 1 << 20,
        },
        |_, _| panic!("actual empty responses"),
    )
    .unwrap();
    let physical_cache = recover_cognition_head_cache_v1(
        refs.caches[0].unwrap(),
        &mut ctx.values,
        &mut hash_work(),
        || false,
    )
    .unwrap();
    let head = CandidateCognitionHeadSourceBuilderV3::genesis(
        request,
        &selected,
        &journal,
        memory,
        || false,
    )
    .unwrap()
    .finish(&physical_cache, &mut hash_work(), || false)
    .unwrap();
    let journal_source = CandidateCognitionHistoryIndexBuilderV3::new(
        request,
        &selected,
        CognitionHistoryFieldV1::JournalEvents,
        Some(&journal),
        None,
        memory,
        || false,
    )
    .unwrap()
    .finish(|| false)
    .unwrap();
    let response_source = CandidateCognitionHistoryIndexBuilderV3::new(
        request,
        &selected,
        CognitionHistoryFieldV1::Responses,
        Some(&responses),
        None,
        memory,
        || false,
    )
    .unwrap()
    .finish(|| false)
    .unwrap();
    let binding = bind_candidate_cognition_history_current_v3(
        request,
        &selected,
        roots,
        current.cognition().history_source(),
        ctx,
    )
    .unwrap();
    let lifecycle =
        CandidateCognitionLifecycleBuilderV3::new(request, &selected, None, memory, || false)
            .unwrap()
            .finish(|| false)
            .unwrap();
    qualify_checked_cognition_history_source_v3(
        CheckedCognitionHistoryComponentsV3 {
            selected,
            current: binding,
            indexes: [Some(journal_source), Some(response_source), None, None],
            heads: [Some(head), None, None],
            lifecycle: Some(lifecycle),
            disabled: None,
        },
        memory,
        || false,
    )
    .unwrap()
}
