//! Closed selected proposal DTO recovery; original JSON is never a DTO blob.
use super::*;
use crate::{
    Manifest, ManifestPatch, ManifestPatchOp, Proposal, ProposalStatus,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_json_records::{
        JsonReadWorkV1, materialize_records, qualify_json_subtree_records_v1,
    },
    incremental_record_columns_read::{
        self as col, ChargedColumnsV1, ColumnCellV1 as Cell, ColumnKeyV1 as Key,
        ColumnReadProfileV1, RecoveredColumnsV1,
    },
};
use oasis7_formal_protocol::{
    incremental_state::{MerkleNodeReader, StateScanBudget},
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::MemoryReservation;
pub type RecoveredRuntimeProposalV1 = RecoveredColumnsV1<Proposal>;
pub(crate) fn reference_schema(key: &Key) -> Option<[u8; 32]> {
    match key {
        Key::Text(name) if name == "manifest.content" => Some(json_profile().schema),
        Key::Pair(a, b)
            if matches!(a.as_ref(),Key::Text(v)if v=="patch.ops")
                && matches!(b.as_ref(),Key::Pair(i,field)if matches!(i.as_ref(),Key::Ordinal(_))&&matches!(field.as_ref(),Key::Text(v)if v=="value")) =>
        {
            Some(json_profile().schema)
        }
        _ => None,
    }
}
pub(crate) fn op_key(index: u64, field: &str) -> Key {
    Key::pair(
        Key::text("patch.ops"),
        Key::pair(Key::Ordinal(index), Key::text(field)),
    )
}
fn path_key(op: u64, index: u64) -> Key {
    Key::pair(
        Key::text("patch.ops"),
        Key::pair(
            Key::Ordinal(op),
            Key::pair(Key::text("path"), Key::Ordinal(index)),
        ),
    )
}
fn spend(work: &mut JsonReadWorkV1) -> Result<(), ValueTreeError> {
    work.cells_remaining = work
        .cells_remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("proposal DTO cells"))?;
    Ok(())
}
fn count(value: u64, remaining: usize) -> Result<usize, ValueTreeError> {
    let value = usize::try_from(value).map_err(|_| invalid())?;
    if value > remaining {
        return Err(invalid());
    }
    Ok(value)
}
fn unsigned_at(body: &mut ChargedColumnsV1, key: &Key) -> Result<u64, ValueTreeError> {
    match body.cells_mut().remove(key).ok_or_else(invalid)? {
        Cell::U64(v) => Ok(v),
        _ => Err(invalid()),
    }
}
fn text_at(body: &mut ChargedColumnsV1, key: &Key) -> Result<String, ValueTreeError> {
    match body.cells_mut().remove(key).ok_or_else(invalid)? {
        Cell::Text(v) => Ok(v),
        _ => Err(invalid()),
    }
}
fn json<M, R, E, C>(
    body: &mut ChargedColumnsV1,
    key: &Key,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut JsonReadWorkV1,
    max_heap: usize,
    charges: &mut Vec<MemoryReservation>,
) -> Result<serde_json::Value, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.check()?;
    let Cell::Root(root) = body.cells_mut().remove(key).ok_or_else(invalid)? else {
        return Err(invalid());
    };
    let profile = json_profile();
    let proof = qualify_json_subtree_records_v1(profile, &root, &mut ctx.records(), work)?;
    let (value, charge) = materialize_records(profile, &proof, &mut ctx.records(), work, max_heap)?;
    charges.push(charge);
    ctx.check()?;
    Ok(value)
}
pub fn recover_runtime_proposal_body_v1<M, R, E, C>(
    root: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut JsonReadWorkV1,
    max_heap: usize,
) -> Result<RecoveredRuntimeProposalV1, ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut body = col::read_columns_v1(
        ColumnReadProfileV1 {
            field: columns(),
            schema: schema(),
            groups: &[],
            reference_schema: Some(reference_schema),
        },
        root,
        ctx,
        scan,
        max_heap,
    )?;
    let patch_present = col::present(body.cells_mut(), "patch")?;
    let op_count = if patch_present {
        let n = col::unsigned(body.cells_mut(), "patch.ops.count")?;
        count(n, body.cells_mut().len())?
    } else {
        0
    };
    let json_capacity = op_count.checked_add(1).ok_or_else(invalid)?;
    body.spend_output(
        json_capacity
            .checked_mul(std::mem::size_of::<MemoryReservation>() + 32)
            .and_then(|n| {
                op_count
                    .checked_mul(std::mem::size_of::<ManifestPatchOp>() + 64)
                    .and_then(|m| n.checked_add(m))
            })
            .and_then(|n| n.checked_add(std::mem::size_of::<Proposal>() + 512))
            .ok_or_else(invalid)?,
    )?;
    let mut charges = Vec::with_capacity(json_capacity);
    let version = col::unsigned(body.cells_mut(), "manifest.version")?;
    let content = json(
        &mut body,
        &Key::text("manifest.content"),
        ctx,
        work,
        max_heap,
        &mut charges,
    )?;
    let patch = if patch_present {
        let base_manifest_hash = col::text(body.cells_mut(), "patch.base_manifest_hash")?;
        let new_version = col::opt_unsigned(body.cells_mut(), "patch.new_version")?;
        let mut ops = Vec::with_capacity(op_count);
        for ordinal in 0..op_count {
            ctx.check()?;
            spend(work)?;
            let ordinal = u64::try_from(ordinal).map_err(|_| invalid())?;
            let kind = unsigned_at(&mut body, &op_key(ordinal, "kind"))?;
            if kind > 1 {
                return Err(invalid());
            }
            let n = unsigned_at(&mut body, &op_key(ordinal, "path.count"))?;
            let path_count = count(n, body.cells_mut().len())?;
            body.spend_output(
                path_count
                    .checked_mul(std::mem::size_of::<String>())
                    .and_then(|n| n.checked_add(128))
                    .ok_or_else(invalid)?,
            )?;
            let mut path = Vec::with_capacity(path_count);
            for index in 0..path_count {
                ctx.check()?;
                spend(work)?;
                let key = path_key(ordinal, u64::try_from(index).map_err(|_| invalid())?);
                path.push(text_at(&mut body, &key)?);
            }
            let op = match kind {
                0 => ManifestPatchOp::Set {
                    path,
                    value: json(
                        &mut body,
                        &op_key(ordinal, "value"),
                        ctx,
                        work,
                        max_heap,
                        &mut charges,
                    )?,
                },
                1 => ManifestPatchOp::Remove { path },
                _ => return Err(invalid()),
            };
            ctx.check()?;
            ops.push(op);
        }
        Some(ManifestPatch {
            base_manifest_hash,
            ops,
            new_version,
        })
    } else {
        None
    };
    let status = match col::unsigned(body.cells_mut(), "status.kind")? {
        0 => ProposalStatus::Proposed,
        1 => ProposalStatus::Shadowed {
            manifest_hash: col::text(body.cells_mut(), "status.manifest_hash")?,
        },
        2 => ProposalStatus::Approved {
            manifest_hash: col::text(body.cells_mut(), "status.manifest_hash")?,
            approver: col::text(body.cells_mut(), "status.approver")?,
        },
        3 => ProposalStatus::Rejected {
            reason: col::text(body.cells_mut(), "status.reason")?,
        },
        4 => ProposalStatus::Applied {
            manifest_hash: col::text(body.cells_mut(), "status.manifest_hash")?,
        },
        _ => return Err(invalid()),
    };
    let value = Proposal {
        id: col::unsigned(body.cells_mut(), "id")?,
        author: col::text(body.cells_mut(), "author")?,
        base_manifest_hash: col::text(body.cells_mut(), "base_manifest_hash")?,
        manifest: Manifest { version, content },
        patch,
        queued_at_tick: col::opt_unsigned(body.cells_mut(), "queued_at_tick")?,
        not_before_tick: col::opt_unsigned(body.cells_mut(), "not_before_tick")?,
        activate_epoch: col::opt_unsigned(body.cells_mut(), "activate_epoch")?,
        timelock_ticks: col::unsigned(body.cells_mut(), "timelock_ticks")?,
        status,
    };
    ctx.check()?;
    body.finish(value, charges)
}
