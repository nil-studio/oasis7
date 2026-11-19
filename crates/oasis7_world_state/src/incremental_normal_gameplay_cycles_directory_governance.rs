//! Private governance child, using the sole original outer/body/cell codec.
use super::*;
use crate::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_normal_gameplay_cycles_directory::{
        self as directory, GameplayDirectoryKindV1 as K, GameplayDirectoryRowV1,
        GameplayDirectoryWorkV1, GameplaySelectorRefV1 as S,
    },
};
use oasis7_formal_protocol::incremental_state::{MerkleNodeReader, StateEntryV1, StateScanBudget};
use oasis7_formal_protocol::incremental_value::{ValueNodeReader, ValueReadContext};
use oasis7_proto::memory_budget::ReservedValue;
use std::sync::Arc;
fn column<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    name: GovernanceColumnV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut GameplayDirectoryWorkV1,
) -> Result<GovernanceValueV1, ValueTreeError> {
    work.step(0, &mut ctx.cancelled)?;
    let cell = GovernanceCellV1::Column(name);
    let key = read::cell_key(GovernanceRecordKindV1::Proposal, cell, &mut ctx.records())?;
    let value = ctx
        .lookup(&key.0)?
        .ok_or(ValueTreeError::Invalid("gameplay proposal selector absent"))?;
    read::value(
        cell,
        &value,
        &key,
        &mut ctx.records(),
        work.max_payload_bytes,
        &mut std::io::sink(),
    )
}
pub(crate) fn inspect_gameplay_governance_directory_row_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    entry: &StateEntryV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut GameplayDirectoryWorkV1,
) -> Result<Option<Arc<ReservedValue<GameplayDirectoryRowV1>>>, ValueTreeError> {
    let Some((kind, owner, child)) = family::outer(entry, ctx)? else {
        return Ok(None);
    };
    if kind != GovernanceRecordKindV1::Proposal {
        return Ok(None);
    }
    let outer = directory::original_text(entry, ctx, work)?;
    let mut body = WorldFieldReadContextV1 {
        root: &child,
        state: ctx.state,
        values: ValueReadContext {
            source: ctx.values.source,
            memory: ctx.values.memory,
            class: ctx.values.class,
            io: &mut *ctx.values.io,
        },
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        take_state_error: &mut ctx.take_state_error,
        cancelled: &mut ctx.cancelled,
    };
    let _closed = qualify_governance_body_v1(
        kind,
        owner,
        &mut body,
        scan,
        &mut work.visits_remaining,
        work.max_payload_bytes,
    )?;
    let GovernanceValueV1::Unsigned(status) =
        column(GovernanceColumnV1::ProposalStatus, &mut body, work)?
    else {
        return Err(ValueTreeError::Invalid("original proposal status shape"));
    };
    let GovernanceValueV1::Unsigned(closes_at) =
        column(GovernanceColumnV1::ClosesAt, &mut body, work)?
    else {
        return Err(ValueTreeError::Invalid("original proposal close shape"));
    };
    // The original writer's fixed ProposalStatus codec maps Open to ordinal 0.
    let row = directory::retain_row(
        K::Proposal,
        entry,
        &outer,
        S::Proposal {
            open: status == 0,
            closes_at,
        },
        body.values.memory,
        body.values.class,
        work,
        &mut body.cancelled,
    )?;
    body.check()?;
    Ok(Some(row))
}
