//! Bound a cloned StateRoot to the caller's actual Common meter.
use super::*;

/// StateRoot is not serde DATA. Its only child is the actual root string;
/// prepay that exact fixed layout before cloning the original descriptor.
pub(super) fn reserve_state_root_v1(
    root: &oasis7_formal_protocol::incremental_state::StateRootV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<MemoryReservation, BodyMutationError> {
    let n = std::mem::size_of::<oasis7_formal_protocol::incremental_state::StateRootV1>()
        .checked_add(root.node_root.len())
        .ok_or(ValueTreeError::ResourceLimited("Bootstrap StateRoot copy"))?;
    common
        .charge(1, n as u64)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    common
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(|e| ValueTreeError::Memory(e).into())
}
