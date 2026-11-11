//! Child of disabled_v3: fixed output admission precedes formal acceptance.
use super::*;
use oasis7_formal_protocol::incremental_material::{
    PreparedFactPromotionV1, finish_prepared_fact_promotion_v1, prepare_candidate_fact_promotion_v1,
};
use oasis7_proto::memory_budget::MemoryReservation;
pub struct PreparedDisabledCognitionSourcePromotionV3 {
    revision: u64,
    fact: PreparedFactPromotionV1,
    charge: MemoryReservation,
    _metadata: MemoryReservation,
}
pub fn prepare_disabled_cognition_source_promotion_v3(
    checked: &ReservedValue<CheckedDisabledCognitionHistorySourceV3>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<PreparedDisabledCognitionSourcePromotionV3, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !checked.uses_budget(memory)
        || checked.reserved_bytes() < 4096
        || checked.occurrence.kind() != ExecutionFactKindV1::Result
    {
        return Err(invalid());
    }
    let metadata = memory.reserve(
        std::mem::size_of::<PreparedDisabledCognitionSourcePromotionV3>(),
        MemoryClass::Ordinary,
    )?;
    let charge = memory.reserve(4096, MemoryClass::Ordinary)?;
    let fact = prepare_candidate_fact_promotion_v1(&checked.occurrence, memory, &mut cancelled)
        .map_err(source_error)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(PreparedDisabledCognitionSourcePromotionV3 {
        revision: checked.revision,
        fact,
        charge,
        _metadata: metadata,
    })
}
pub fn finish_prepared_disabled_cognition_source_promotion_v3(
    prepared: PreparedDisabledCognitionSourcePromotionV3,
    accepted: &VerifiedFormalCommitV1,
) -> Result<ReservedValue<AcceptedDisabledCognitionHistorySourceV3>, ValueTreeError> {
    let occurrence =
        finish_prepared_fact_promotion_v1(prepared.fact, accepted).map_err(|_| invalid())?;
    if occurrence.kind() != ExecutionFactKindV1::Result
        || occurrence.head_body_id() != occurrence.fact_body_id()
    {
        return Err(invalid());
    }
    Ok(prepared
        .charge
        .retain(AcceptedDisabledCognitionHistorySourceV3 {
            head: occurrence.head_record_id(),
            roots: occurrence.roots_id(),
            body: occurrence.head_body_id(),
            location: occurrence.location(),
            revision: prepared.revision,
            class: MemoryClass::Ordinary,
        }))
}
