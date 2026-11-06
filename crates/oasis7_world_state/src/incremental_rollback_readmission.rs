//! Ordinary admission for an immutable shared cold-recovery result.
use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
/// Re-admit a proof/carrier through the ordinary limit while retaining the
/// complete original DTO carrier and every child outcome reservation. This
/// shares immutable data; it does not relabel or drop its existing charges.
/// The original recovery carrier admission includes its Arc/control block.
pub fn readmit_rollback_controls_ordinary_v1(
    source: &std::sync::Arc<ReservedValue<RecoveredRollbackControlsV1>>,
    memory: &MemoryBudget,
) -> Result<ReservedValue<RecoveredRollbackControlsV1>, ValueTreeError> {
    if !source.uses_budget(memory)
        || source.reserved_bytes() < std::mem::size_of::<RecoveredRollbackControlsV1>()
    {
        return Err(invalid());
    }
    let charge = memory.reserve(1024, MemoryClass::Ordinary)?;
    let original = match &source.data {
        RollbackControlsDataV1::Owned(_) => std::sync::Arc::clone(source),
        RollbackControlsDataV1::Readmitted(original) => std::sync::Arc::clone(original),
    };
    Ok(charge.retain(RecoveredRollbackControlsV1 {
        data: RollbackControlsDataV1::Readmitted(original),
        root_set_digest: source.root_set_digest,
        registry_root: source.registry_root.clone(),
        class: MemoryClass::Ordinary,
    }))
}
