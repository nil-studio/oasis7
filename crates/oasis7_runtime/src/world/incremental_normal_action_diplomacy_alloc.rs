//! Closed Alliance normalization allocations; no caller count/payment witness.
use crate::world::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::mem::size_of;
fn limited() -> BodyMutationError {
    ValueTreeError::ResourceLimited("Alliance normalization layout").into()
}
fn add(a: usize, b: usize) -> Result<usize, BodyMutationError> {
    a.checked_add(b).ok_or_else(limited)
}
fn charge(common: &mut FormalBoundedContextV1<'_>, n: usize) -> Result<(), BodyMutationError> {
    common
        .charge(1, u64::try_from(n).map_err(|_| limited())?)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))
}
/// Original BTreeSet insertion may momentarily retain a duplicate String while
/// its equal stored String is live. Sum every actual input, including duplicates.
/// Into-iteration moves these strings into a Vec while set nodes are still live.
fn normalization_layout(
    proposer: &str,
    members: &[String],
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<usize, BodyMutationError> {
    charge(common, proposer.len())?;
    let mut payload = proposer.trim().len();
    for member in members {
        charge(common, member.len())?;
        payload = add(payload, member.trim().len())?;
    }
    let n = add(members.len(), 1)?;
    let tree = oasis7_world_state::incremental_collection_layout::string_set_output_heap_v1(n)
        .map_err(BodyMutationError::from)?;
    // Vec::from_iter on the BTreeSet iterator may use its exact size; allow the
    // allocator's standard minimum four String slots for a short nonempty result.
    let slots = n
        .max(4)
        .checked_mul(size_of::<String>())
        .ok_or_else(limited)?;
    let layout = add(
        add(add(tree, payload)?, slots)?,
        size_of::<std::collections::BTreeSet<String>>() + size_of::<Vec<String>>(),
    )?;
    // All set comparisons are lexicographic over at most n stored/input strings.
    // n comparisons per insertion is a conservative linear-tree bound, avoiding a
    // claim about the library's balancing shape or logarithmic comparison count.
    charge(common, payload.checked_mul(n).ok_or_else(limited)?)?;
    Ok(layout)
}
pub(super) fn reserve_normalization_v1(
    p: &str,
    m: &[String],
    c: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<()>, BodyMutationError> {
    let n = normalization_layout(p, m, c)?;
    Ok(c.memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?
        .retain(()))
}
pub(super) fn check_normalization_fee_v1(
    p: &str,
    m: &[String],
    fee: &ReservedValue<()>,
    c: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    let n = normalization_layout(p, m, c)?;
    if !fee.uses_budget(c.memory()) || fee.reserved_bytes() < n {
        return Err(limited());
    }
    Ok(())
}
fn former_layout(
    members: &[String],
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<usize, BodyMutationError> {
    let mut payload = 0usize;
    for member in members {
        charge(common, member.len())?;
        payload = add(payload, member.len())?;
    }
    let layout = add(
        add(
            payload,
            members
                .len()
                .checked_mul(size_of::<String>())
                .ok_or_else(limited)?,
        )?,
        size_of::<Vec<String>>(),
    )?;
    Ok(layout)
}
pub(super) fn reserve_former_members_v1(
    m: &[String],
    c: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<()>, BodyMutationError> {
    let n = former_layout(m, c)?;
    Ok(c.memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?
        .retain(()))
}
pub(super) fn check_former_members_fee_v1(
    m: &[String],
    fee: &ReservedValue<()>,
    c: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    let n = former_layout(m, c)?;
    if !fee.uses_budget(c.memory()) || fee.reserved_bytes() < n {
        return Err(limited());
    }
    Ok(())
}
