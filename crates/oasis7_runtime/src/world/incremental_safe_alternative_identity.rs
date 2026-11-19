//! Actual original identity resolution, sharing the production-order point readers.
//! This module is nested under the fixed industrial owner, not a caller adapter.
use super::*;
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn read_current_safe_alternative_identity_v1<M, R, E, C, P, V, ER, CR>(
    sources: &IndustrialEligibilitySourcesV1<'_, '_>,
    agent: &str,
    state: &M,
    overlay: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent_read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
    work: &mut IndustrialEligibilityWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<Option<crate::CapabilityAgentIdentity>>, IndustrialEligibilityErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    CR: FnMut() -> bool,
{
    with_current_industrial_identity_v1(
        sources,
        agent,
        state,
        overlay,
        parent_read,
        work,
        guard,
        live,
        |value, memory, work| {
            let Some((owner, generation)) = value else {
                let fee = memory
                    .reserve(
                        std::mem::size_of::<Option<crate::CapabilityAgentIdentity>>(),
                        MemoryClass::Ordinary,
                    )
                    .map_err(ValueTreeError::Memory)?;
                return Ok(fee.retain(None));
            };
            work.read.comparison_bytes_remaining = work
                .read
                .comparison_bytes_remaining
                .checked_sub(owner.len() as u64)
                .ok_or(ValueTreeError::ResourceLimited(
                    "safe alternative identity copy",
                ))?;
            if owner.len() as u64 > work.read.max_text_bytes {
                return Err(
                    ValueTreeError::ResourceLimited("safe alternative identity text").into(),
                );
            }
            let layout = std::mem::size_of::<Option<crate::CapabilityAgentIdentity>>()
                .checked_add(owner.len())
                .ok_or(ValueTreeError::ResourceLimited(
                    "safe alternative identity layout",
                ))?;
            let fee = memory
                .reserve(layout, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let mut text = String::with_capacity(owner.len());
            if text.capacity() > owner.len() {
                return Err(
                    ValueTreeError::ResourceLimited("safe alternative identity capacity").into(),
                );
            }
            text.push_str(owner);
            Ok(fee.retain(Some(crate::CapabilityAgentIdentity {
                owner_binding: text,
                generation,
            })))
        },
    )
}
