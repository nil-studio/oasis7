//! Child of source_candidate: copying/validating the fixed cache is preaccept.
use super::*;
use oasis7_formal_protocol::incremental_material::{
    PreparedFactPromotionV1, finish_prepared_fact_promotion_v1, prepare_candidate_fact_promotion_v1,
};
use oasis7_proto::memory_budget::MemoryReservation;
pub struct PreparedCognitionHeadSourcePromotionV3 {
    state: ReservedValue<CognitionHeadHashStateV1>,
    field: CognitionHistoryFieldV1,
    records: u64,
    last: Option<[u8; 32]>,
    last_record: Option<CognitionHeadSourceLastV3>,
    fact: PreparedFactPromotionV1,
    charge: MemoryReservation,
    _metadata: MemoryReservation,
}
pub fn prepare_cognition_head_source_promotion_v3(
    checked: &ReservedValue<CheckedCandidateHeadSourceV3>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<PreparedCognitionHeadSourcePromotionV3, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !checked.uses_budget(memory)
        || checked.reserved_bytes() < 4096
        || !checked.state.uses_budget(memory)
        || checked.state.allocation_class() != MemoryClass::Ordinary
        || checked.state.reserved_bytes() < CognitionHeadHashBuilderV1::admission_bytes()
        || checked.occurrence.kind() != ExecutionFactKindV1::Result
    {
        return Err(candidate_invalid());
    }
    let metadata = memory.reserve(
        std::mem::size_of::<PreparedCognitionHeadSourcePromotionV3>(),
        MemoryClass::Ordinary,
    )?;
    let charge = memory.reserve(4096, MemoryClass::Ordinary)?;
    let fact = prepare_candidate_fact_promotion_v1(&checked.occurrence, memory, &mut cancelled)
        .map_err(candidate_core_error)?;
    let state = CognitionHeadHashBuilderV1::append_to(
        &checked.state,
        memory,
        MemoryClass::Ordinary,
        &mut cancelled,
    )?
    .finish(&mut cancelled)?;
    let last_record = checked.last_record.clone();
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(PreparedCognitionHeadSourcePromotionV3 {
        state,
        field: checked.field,
        records: checked.records,
        last: checked.last,
        last_record,
        fact,
        charge,
        _metadata: metadata,
    })
}
pub fn finish_prepared_cognition_head_source_promotion_v3(
    prepared: PreparedCognitionHeadSourcePromotionV3,
    accepted: &VerifiedFormalCommitV1,
) -> Result<ReservedValue<AuthenticatedCognitionHeadSourceV1>, ValueTreeError> {
    let occurrence = finish_prepared_fact_promotion_v1(prepared.fact, accepted)
        .map_err(|_| candidate_invalid())?;
    if occurrence.kind() != ExecutionFactKindV1::Result
        || occurrence.head_body_id() != occurrence.fact_body_id()
    {
        return Err(candidate_invalid());
    }
    Ok(prepared.charge.retain(AuthenticatedCognitionHeadSourceV1 {
        state: prepared.state,
        field: prepared.field,
        head: occurrence.head_record_id(),
        roots: occurrence.roots_id(),
        records: prepared.records,
        last: prepared.last,
        last_record: prepared.last_record,
    }))
}
