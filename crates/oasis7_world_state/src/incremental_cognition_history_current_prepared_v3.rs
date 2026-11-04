//! Child of candidate_current_v3; never constructs an accepted binding early.
use super::*;
use oasis7_formal_protocol::incremental_material::{
    PreparedFactPromotionV1, finish_prepared_fact_promotion_v1, prepare_candidate_fact_promotion_v1,
};
use oasis7_proto::memory_budget::MemoryReservation;
pub struct PreparedCognitionHistoryCurrentPromotionV3 {
    enabled: bool,
    fact: PreparedFactPromotionV1,
    charge: MemoryReservation,
    _metadata: MemoryReservation,
}
pub fn prepare_cognition_history_current_promotion_v3(
    checked: &ReservedValue<CheckedCognitionHistoryCurrentBindingV3>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<PreparedCognitionHistoryCurrentPromotionV3, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !checked.uses_budget(memory)
        || checked.reserved_bytes() < 8192
        || checked.occurrence.kind() != ExecutionFactKindV1::Result
    {
        return Err(invalid());
    }
    let metadata = memory.reserve(
        std::mem::size_of::<PreparedCognitionHistoryCurrentPromotionV3>(),
        MemoryClass::Ordinary,
    )?;
    let charge = memory.reserve(8192, MemoryClass::Ordinary)?;
    let fact = prepare_candidate_fact_promotion_v1(&checked.occurrence, memory, &mut cancelled)
        .map_err(source_error)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(PreparedCognitionHistoryCurrentPromotionV3 {
        enabled: checked.enabled,
        fact,
        charge,
        _metadata: metadata,
    })
}
pub fn finish_prepared_cognition_history_current_promotion_v3(
    prepared: PreparedCognitionHistoryCurrentPromotionV3,
    accepted: &VerifiedFormalCommitV1,
) -> Result<ReservedValue<CognitionHistoryCurrentBindingV1>, ValueTreeError> {
    let occurrence =
        finish_prepared_fact_promotion_v1(prepared.fact, accepted).map_err(|_| invalid())?;
    if occurrence.kind() != ExecutionFactKindV1::Result
        || occurrence.head_body_id() != occurrence.fact_body_id()
    {
        return Err(invalid());
    }
    Ok(prepared.charge.retain(CognitionHistoryCurrentBindingV1 {
        roots: occurrence.roots_id(),
        head: occurrence.head_record_id(),
        enabled: prepared.enabled,
        class: MemoryClass::Ordinary,
    }))
}
