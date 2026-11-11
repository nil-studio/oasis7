//! Explicit resource-only Recovery to Ordinary metadata readmission.
use super::*;
pub fn readmit_authorization_record_families_ordinary_v1(
    source: &ReservedValue<AuthorizationRecordFamiliesQualificationV1>,
    roots: &RootSetV1,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<AuthorizationRecordFamiliesQualificationV1>, ValueTreeError> {
    use oasis7_proto::memory_budget::MemoryClass;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !source.uses_budget(memory)
        || source.reserved_bytes() < 512
        || source.class != MemoryClass::Recovery
    {
        return Err(invalid());
    }
    let mut scratch = memory.reserve(128 << 10, MemoryClass::Ordinary)?;
    source.validate_binding(roots)?;
    let result = scratch.split(512).map_err(ValueTreeError::Invalid)?.retain(
        AuthorizationRecordFamiliesQualificationV1 {
            roots: source.roots,
            counts: source.counts,
            class: MemoryClass::Ordinary,
        },
    );
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(result)
}
