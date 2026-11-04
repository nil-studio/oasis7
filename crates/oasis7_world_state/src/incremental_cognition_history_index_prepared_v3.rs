//! Child of candidate_index_v3; root String allocation is preacceptance only.
use super::*;
use oasis7_formal_protocol::incremental_material::{
    PreparedFactPromotionV1, finish_prepared_fact_promotion_v1, prepare_candidate_fact_promotion_v1,
};
use oasis7_proto::memory_budget::MemoryReservation;
pub struct PreparedCognitionHistoryIndexPromotionV3 {
    root: StateRootV1,
    field: CognitionHistoryFieldV1,
    records: u64,
    fact: PreparedFactPromotionV1,
    charge: MemoryReservation,
    _metadata: MemoryReservation,
}
pub fn prepare_cognition_history_index_promotion_v3(
    checked: &ReservedValue<CheckedCognitionHistoryIndexSourceV3>,
    selected: &ReservedValue<CheckedCognitionHistoryViewV1>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<PreparedCognitionHistoryIndexPromotionV3, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let declared = selected.manifest().index_ref(checked.field);
    if !checked.uses_budget(memory)
        || checked.reserved_bytes() < 16 << 10
        || !selected.uses_budget(memory)
        || selected.reserved_bytes() < 16 << 10
        || !selected.occurrence().uses_budget(memory)
        || selected.occurrence().reserved_bytes()
            < std::mem::size_of::<CheckedCandidateFactOccurrenceV1>()
        || checked.occurrence.kind() != ExecutionFactKindV1::Result
        || !declared.present
        || checked.root != declared.root
        || checked.records != declared.records
        || checked.occurrence.candidate_record_id() != selected.occurrence().candidate_record_id()
        || checked.occurrence.body_id() != selected.occurrence().body_id()
        || checked.occurrence.roots_id() != selected.manifest().roots
        || checked.occurrence.location() != selected.occurrence().location()
    {
        return Err(invalid_candidate_index());
    }
    let metadata = memory.reserve(
        std::mem::size_of::<PreparedCognitionHistoryIndexPromotionV3>(),
        MemoryClass::Ordinary,
    )?;
    let charge = memory.reserve(16 << 10, MemoryClass::Ordinary)?;
    let fact = prepare_candidate_fact_promotion_v1(&checked.occurrence, memory, &mut cancelled)
        .map_err(core_error)?;
    let root = checked.root.clone();
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(PreparedCognitionHistoryIndexPromotionV3 {
        root,
        field: checked.field,
        records: checked.records,
        fact,
        charge,
        _metadata: metadata,
    })
}
pub fn finish_prepared_cognition_history_index_promotion_v3(
    prepared: PreparedCognitionHistoryIndexPromotionV3,
    accepted: &VerifiedFormalCommitV1,
) -> Result<ReservedValue<AttestedCognitionHistoryIndexV3>, ValueTreeError> {
    let occurrence = finish_prepared_fact_promotion_v1(prepared.fact, accepted)
        .map_err(|_| invalid_candidate_index())?;
    if occurrence.kind() != ExecutionFactKindV1::Result
        || occurrence.head_body_id() != occurrence.fact_body_id()
    {
        return Err(invalid_candidate_index());
    }
    Ok(prepared.charge.retain(AttestedCognitionHistoryIndexV3 {
        root: prepared.root,
        field: prepared.field,
        records: prepared.records,
        head: occurrence.head_record_id(),
        roots: occurrence.roots_id(),
        class: MemoryClass::Ordinary,
    }))
}
