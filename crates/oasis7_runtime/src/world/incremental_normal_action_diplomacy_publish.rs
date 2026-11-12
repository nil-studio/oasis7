//! Fixed Alliance membership replacement, retaining the authenticated child.
//! Private storage helper: its returned proof is data, never an action permit.
use crate::world::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::{
    AllianceState,
    incremental_diplomacy::*,
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
};
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("Alliance Current member replacement")
}
fn overlay(e: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match e {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        WorldFieldOverlayErrorV1::Member(e) => BodyMutationError::Original(Box::new(e)),
    }
}
fn charge(common: &mut FormalBoundedContextV1<'_>, n: usize) -> Result<(), BodyMutationError> {
    common
        .charge(
            1,
            u64::try_from(n)
                .map_err(|_| ValueTreeError::ResourceLimited("Alliance replacement work"))?,
        )
        .map_err(|e| BodyMutationError::Original(Box::new(e)))
}
#[allow(clippy::too_many_arguments)]
pub(super) fn replace_members_v1<M, R, E, C, W, CW>(
    id: &str,
    before: &AllianceState,
    after: &[String],
    mut proof: ReservedValue<DiplomacyRecordQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    max_payload: u64,
    max_pending: usize,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<ReservedValue<DiplomacyRecordQualificationV1>, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
{
    let mut length = before.members.len();
    // Replace the shared prefix before deleting the old suffix. No ordering or
    // embedded ID assumption is made about the original stored member sequence.
    for ordinal in 0..before.members.len().min(after.len()) {
        charge(
            common,
            id.len()
                .checked_add(before.members[ordinal].len())
                .and_then(|n| n.checked_add(after[ordinal].len()))
                .ok_or_else(bad)?,
        )?;
        if before.members[ordinal] == after[ordinal] {
            continue;
        }
        let guard = ctx.transaction.guard();
        let next = change_qualified_diplomacy_member_v1(
            DiplomacyMemberChangeV1 {
                field: DiplomacyFieldV1::Alliances,
                id,
                qualification: &proof,
                expected_length: length as u64,
                ordinal: ordinal as u64,
                expected: Some(DiplomacyMemberV1::Alliance(&before.members[ordinal])),
                after: Some(DiplomacyMemberV1::Alliance(&after[ordinal])),
                max_current_payload_bytes: max_payload,
                max_pending_node_bytes: max_pending,
            },
            ctx,
            write,
            &guard,
            live,
        )
        .map_err(overlay)?;
        proof = next.ok_or_else(bad)?;
    }
    for ordinal in (after.len()..before.members.len()).rev() {
        charge(
            common,
            id.len()
                .checked_add(before.members[ordinal].len())
                .ok_or_else(bad)?,
        )?;
        let guard = ctx.transaction.guard();
        let next = change_qualified_diplomacy_member_v1(
            DiplomacyMemberChangeV1 {
                field: DiplomacyFieldV1::Alliances,
                id,
                qualification: &proof,
                expected_length: length as u64,
                ordinal: ordinal as u64,
                expected: Some(DiplomacyMemberV1::Alliance(&before.members[ordinal])),
                after: None,
                max_current_payload_bytes: max_payload,
                max_pending_node_bytes: max_pending,
            },
            ctx,
            write,
            &guard,
            live,
        )
        .map_err(overlay)?;
        proof = next.ok_or_else(bad)?;
        length -= 1;
    }
    for ordinal in before.members.len()..after.len() {
        charge(
            common,
            id.len().checked_add(after[ordinal].len()).ok_or_else(bad)?,
        )?;
        let guard = ctx.transaction.guard();
        let next = change_qualified_diplomacy_member_v1(
            DiplomacyMemberChangeV1 {
                field: DiplomacyFieldV1::Alliances,
                id,
                qualification: &proof,
                expected_length: length as u64,
                ordinal: ordinal as u64,
                expected: None,
                after: Some(DiplomacyMemberV1::Alliance(&after[ordinal])),
                max_current_payload_bytes: max_payload,
                max_pending_node_bytes: max_pending,
            },
            ctx,
            write,
            &guard,
            live,
        )
        .map_err(overlay)?;
        proof = next.ok_or_else(bad)?;
        length += 1;
    }
    Ok(proof)
}
