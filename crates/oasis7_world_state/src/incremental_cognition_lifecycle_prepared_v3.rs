//! Child of lifecycle_candidate_v3: both owned root Strings precede acceptance.
use super::*;
use oasis7_formal_protocol::incremental_material::{
    PreparedFactPromotionV1, finish_prepared_fact_promotion_v1, prepare_candidate_fact_promotion_v1,
};
use oasis7_proto::memory_budget::MemoryReservation;
pub struct PreparedCognitionLifecyclePromotionV3 {
    root: StateRootV1,
    journal: StateRootV1,
    keys: u64,
    records: u64,
    fact: PreparedFactPromotionV1,
    charge: MemoryReservation,
    _metadata: MemoryReservation,
}
pub fn prepare_cognition_lifecycle_promotion_v3(
    checked: &ReservedValue<CheckedCognitionLifecycleV3>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<PreparedCognitionLifecyclePromotionV3, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !checked.uses_budget(memory)
        || checked.reserved_bytes() < 16 << 10
        || checked.occurrence.kind() != ExecutionFactKindV1::Result
    {
        return Err(invalid());
    }
    let metadata = memory.reserve(
        std::mem::size_of::<PreparedCognitionLifecyclePromotionV3>(),
        MemoryClass::Ordinary,
    )?;
    let charge = memory.reserve(4096, MemoryClass::Ordinary)?;
    let fact = prepare_candidate_fact_promotion_v1(&checked.occurrence, memory, &mut cancelled)
        .map_err(candidate_error)?;
    let root = checked.root.clone();
    let journal = checked.journal.clone();
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(PreparedCognitionLifecyclePromotionV3 {
        root,
        journal,
        keys: checked.keys,
        records: checked.records,
        fact,
        charge,
        _metadata: metadata,
    })
}
pub fn finish_prepared_cognition_lifecycle_promotion_v3(
    prepared: PreparedCognitionLifecyclePromotionV3,
    accepted: &VerifiedFormalCommitV1,
) -> Result<ReservedValue<CognitionLifecycleQualificationV2>, ValueTreeError> {
    let occurrence =
        finish_prepared_fact_promotion_v1(prepared.fact, accepted).map_err(|_| invalid())?;
    if occurrence.kind() != ExecutionFactKindV1::Result
        || occurrence.head_body_id() != occurrence.fact_body_id()
    {
        return Err(invalid());
    }
    Ok(prepared.charge.retain(CognitionLifecycleQualificationV2 {
        root: prepared.root,
        keys: prepared.keys,
        journal: prepared.journal,
        records: prepared.records,
        head: occurrence.head_record_id(),
        roots: occurrence.roots_id(),
        class: MemoryClass::Ordinary,
    }))
}
