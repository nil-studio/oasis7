//! Explicit resource bridge, never a format/profile or selected-head upgrade.
use super::*;
pub fn readmit_authenticated_cognition_head_source_ordinary_v1(
    parent: &ReservedValue<AuthenticatedCognitionHeadSourceV1>,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<AuthenticatedCognitionHeadSourceV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !parent.uses_budget(memory)
        || parent.reserved_bytes() < 4096
        || !parent.state.uses_budget(memory)
        || parent.state.allocation_class() != MemoryClass::Recovery
    {
        return Err(ValueTreeError::Invalid(
            "head source readmission class/pool",
        ));
    }
    // Both admissions precede their independent copies; no Recovery Arc is
    // retained by the returned Ordinary proof.
    let charge = memory.reserve(4096, MemoryClass::Ordinary)?;
    let state = readmit_cognition_head_cache_ordinary_v1(&parent.state, memory, &mut cancelled)?;
    let result = AuthenticatedCognitionHeadSourceV1 {
        state,
        field: parent.field,
        head: parent.head,
        roots: parent.roots,
        records: parent.records,
        last: parent.last,
        last_record: parent.last_record.clone(),
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(charge.retain(result))
}
