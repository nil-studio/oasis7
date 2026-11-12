//! Paid exact original Join/Leave member projections, preserving legacy rows.
use super::facts::AllianceEvaluationErrorV1;
use crate::world::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::AllianceState;
use std::mem::size_of;
#[derive(Clone, Copy)]
pub(super) enum MemberOperationV1 {
    Join,
    Leave,
}
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("Alliance member projection")
}
fn limited() -> BodyMutationError {
    ValueTreeError::ResourceLimited("Alliance member projection layout").into()
}
fn charge(common: &mut FormalBoundedContextV1<'_>, n: usize) -> Result<(), BodyMutationError> {
    common
        .charge(1, u64::try_from(n).map_err(|_| limited())?)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))
}
pub(super) fn project_members_v1(
    outer: &str,
    before: &AllianceState,
    member: &str,
    operation: MemberOperationV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<Vec<String>>, AllianceEvaluationErrorV1> {
    let n = before
        .members
        .len()
        .checked_add(usize::from(matches!(operation, MemberOperationV1::Join)))
        .ok_or_else(limited)?;
    let mut payload = if matches!(operation, MemberOperationV1::Join) {
        member.len()
    } else {
        0
    };
    let mut longest = member.len();
    for value in &before.members {
        charge(common, value.len())?;
        payload = payload.checked_add(value.len()).ok_or_else(limited)?;
        longest = longest.max(value.len());
    }
    // Original stable sort may hold a temporary String slot array; both arrays
    // and every cloned payload are admitted before the clone/append occurs.
    let layout = n
        .checked_mul(2 * size_of::<String>())
        .and_then(|n| n.checked_add(payload))
        .and_then(|n| n.checked_add(size_of::<Vec<String>>()))
        .and_then(|n| {
            n.checked_add(
                outer
                    .len()
                    .checked_add("alliance  member count below minimum 2".len())?
                    .checked_mul(2)?,
            )
        })
        .ok_or_else(limited)?;
    let fee = common
        .memory()
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)
        .map_err(BodyMutationError::from)?;
    let mut after = Vec::with_capacity(n);
    if after.capacity() > n {
        return Err(limited().into());
    }
    for value in &before.members {
        charge(common, value.len())?;
        let text = value.clone();
        if text.capacity() > value.len() {
            return Err(limited().into());
        }
        after.push(text);
    }
    match operation {
        MemberOperationV1::Join => {
            charge(common, member.len())?;
            let text = member.to_owned();
            if text.capacity() > member.len() {
                return Err(limited().into());
            }
            after.push(text);
            // n^2 bounds all comparisons of the original sort plus dedup even for
            // pre-existing unsorted duplicates; only the selected Alliance is visited.
            charge(
                common,
                n.checked_mul(n)
                    .and_then(|n| n.checked_mul(longest.checked_mul(2)?))
                    .ok_or_else(limited)?,
            )?;
            after.sort();
            after.dedup();
        }
        MemberOperationV1::Leave => {
            let mut i = 0;
            while i < after.len() {
                charge(
                    common,
                    after[i]
                        .len()
                        .checked_add(member.len())
                        .ok_or_else(limited)?,
                )?;
                if after[i] == member {
                    charge(
                        common,
                        (after.len() - i)
                            .checked_mul(size_of::<String>())
                            .ok_or_else(limited)?,
                    )?;
                    after.remove(i);
                } else {
                    i += 1;
                }
            }
            if after.len() < 2 {
                charge(common, outer.len())?;
                return Err(crate::WorldError::ResourceBalanceInvalid {
                    reason: format!("alliance {outer} member count below minimum 2"),
                }
                .into());
            }
        }
    }
    Ok(fee.retain(after))
}
