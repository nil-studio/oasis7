//! Child of selection: all owned selection copies precede formal acceptance.
use super::*;
use oasis7_formal_protocol::incremental_material::{
    PreparedFactPromotionV1, finish_prepared_fact_promotion_v1, prepare_candidate_fact_promotion_v1,
};
use oasis7_proto::memory_budget::MemoryReservation;
pub struct PreparedCognitionHistoryViewPromotionV3 {
    manifest: ReservedValue<CognitionHistoryViewV1>,
    derived: Option<CognitionHistoryDerivedRefsV2>,
    source_v3: Option<ReservedValue<Box<CognitionHistorySourceSummaryV3>>>,
    fact: PreparedFactPromotionV1,
    charge: MemoryReservation,
    _metadata: MemoryReservation,
}
fn prepared_error(error: CoreError<Infallible>) -> ValueTreeError {
    use oasis7_formal_protocol::incremental_execution::ExecutionError;
    match error {
        CoreError::Cancelled => ValueTreeError::Cancelled,
        CoreError::ResourceLimited(reason)
        | CoreError::Execution(ExecutionError::ResourceLimited(reason)) => {
            ValueTreeError::ResourceLimited(reason)
        }
        CoreError::Execution(ExecutionError::Memory(error)) => ValueTreeError::Memory(error),
        _ => ValueTreeError::Invalid("prepared Cognition view fact"),
    }
}
pub fn prepare_cognition_history_view_promotion_v3(
    checked: &ReservedValue<CheckedCognitionHistoryViewV1>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<PreparedCognitionHistoryViewPromotionV3, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !checked.uses_budget(memory)
        || checked.reserved_bytes() < 16 << 10
        || !checked.manifest.uses_budget(memory)
        || checked.manifest.reserved_bytes() < 8192
        || checked.occurrence.kind() != ExecutionFactKindV1::Result
        || checked.occurrence.roots_id() != checked.manifest.roots
    {
        return Err(ValueTreeError::Invalid("prepared Cognition view source"));
    }
    let metadata = memory.reserve(
        std::mem::size_of::<PreparedCognitionHistoryViewPromotionV3>(),
        MemoryClass::Ordinary,
    )?;
    let charge = memory.reserve(16 << 10, MemoryClass::Ordinary)?;
    let manifest_charge = memory.reserve(8192, MemoryClass::Ordinary)?;
    let fact = prepare_candidate_fact_promotion_v1(&checked.occurrence, memory, &mut cancelled)
        .map_err(prepared_error)?;
    let manifest = manifest_charge.retain((*checked.manifest).clone());
    let derived = checked.derived.clone();
    let source_v3 = copy_source_v3(&checked.source_v3, memory, MemoryClass::Ordinary)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(PreparedCognitionHistoryViewPromotionV3 {
        manifest,
        derived,
        source_v3,
        fact,
        charge,
        _metadata: metadata,
    })
}
/// Pure refinement: no admission, clone, IO, hashing or cancellation callback.
pub fn finish_prepared_cognition_history_view_promotion_v3(
    prepared: PreparedCognitionHistoryViewPromotionV3,
    accepted: &VerifiedFormalCommitV1,
) -> Result<ReservedValue<AcceptedCognitionHistoryViewV1>, ValueTreeError> {
    let occurrence = finish_prepared_fact_promotion_v1(prepared.fact, accepted)
        .map_err(|_| ValueTreeError::Invalid("prepared Cognition view accepted head"))?;
    if occurrence.kind() != ExecutionFactKindV1::Result
        || occurrence.roots_id() != prepared.manifest.roots
        || occurrence.head_body_id() != occurrence.fact_body_id()
    {
        return Err(ValueTreeError::Invalid(
            "prepared Cognition view accepted binding",
        ));
    }
    Ok(prepared.charge.retain(AcceptedCognitionHistoryViewV1 {
        manifest: prepared.manifest,
        derived: prepared.derived,
        source_v3: prepared.source_v3,
        occurrence,
        class: MemoryClass::Ordinary,
    }))
}
