//! Explicit resource admission for immutable structural family evidence.
use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
pub fn readmit_runtime_effect_family_ordinary_v1(
    source: &ReservedValue<RuntimeEffectFamilyQualificationV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<RuntimeEffectFamilyQualificationV1>, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if !source.uses_budget(memory)
        || source.reserved_bytes() < 512
        || source.class != MemoryClass::Recovery
    {
        return Err(invalid());
    }
    let _phase = memory.reserve(128 << 10, MemoryClass::Ordinary)?;
    source.validate_binding(&roots.digest().map_err(|_| invalid())?)?;
    let result =
        memory
            .reserve(512, MemoryClass::Ordinary)?
            .retain(RuntimeEffectFamilyQualificationV1 {
                roots: source.roots,
                frontier: source.frontier,
                counts: source.counts,
                class: MemoryClass::Ordinary,
            });
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(result)
}
