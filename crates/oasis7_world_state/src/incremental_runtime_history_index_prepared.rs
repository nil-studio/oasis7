//! Private child of incremental_runtime_history_index: prepare the next
//! independent append baseline before accepting its enclosing selected Result.
use super::*;

/// Structural Ordinary output only, not accepted provenance. The enclosing
/// prepared source retains a real candidate-fact promotion for that authority.
/// Resetting append lineage is essential: the next commit must extend this
/// window's root, rather than inherit the previous commit's append baseline.
pub(crate) fn prepare_runtime_history_index_promotion_v1(
    source: &ReservedValue<RuntimeHistoryIndexQualificationV1>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<RuntimeHistoryIndexQualificationV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !source.uses_budget(memory)
        || source.reserved_bytes() < CARRIER
        || source.class != MemoryClass::Ordinary
    {
        return Err(invalid());
    }
    source.window.validate()?;
    // A validated StateRoot owns one fixed 64-byte lowercase hexadecimal
    // String. Admit its new carrier and that clone before cloning any byte.
    let charge = memory.reserve(CARRIER, MemoryClass::Ordinary)?;
    let window = source.window.clone();
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(RuntimeHistoryIndexQualificationV1 {
        lane: source.lane,
        window,
        class: MemoryClass::Ordinary,
        lineage: None,
    }))
}
