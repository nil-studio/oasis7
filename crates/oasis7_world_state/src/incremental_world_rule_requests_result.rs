//! Only authenticated changed registrations feed the resident contract directory.
use super::*;
use crate::{
    incremental_record_value_read::{
        RecordReadLabelsV1, RecordReadProfileV1, RecordValueV1, read_record_value_v1,
    },
    incremental_typed_record_diff::{
        TypedCellRefV1, key, point_typed_column_v1, point_typed_entry_v1, root, visit_entries,
    },
};
#[path = "incremental_world_rule_requests_result_index.rs"]
pub(crate) mod index;
use index::{Builder, RequestsResultIndexV1};

pub(super) fn spend(
    scan: &mut StateScanBudget,
    stop: &mut impl FnMut() -> bool,
    steps: u64,
    bytes: u64,
) -> Result<(), ValueTreeError> {
    if stop() {
        return Err(ValueTreeError::Cancelled);
    }
    scan.nodes_remaining =
        scan.nodes_remaining
            .checked_sub(steps)
            .ok_or(ValueTreeError::ResourceLimited(
                "organization resident metadata work",
            ))?;
    scan.read_bytes_remaining =
        scan.read_bytes_remaining
            .checked_sub(bytes)
            .ok_or(ValueTreeError::ResourceLimited(
                "organization resident metadata bytes",
            ))?;
    Ok(())
}
pub(super) fn update<
    T: CompositeBranchCodecV1,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: Option<&RequestsResultIndexV1>,
    before: Option<&StateRootV1>,
    after: Option<&StateRootV1>,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
) -> Result<Option<RequestsResultIndexV1>, ValueTreeError> {
    if T::BRANCH != WorldRuleCompositeBranchV1::Requests {
        return Ok(None);
    }
    if before == after
        && let Some(index) = parent
    {
        return Ok(Some(index.share(ctx.values.memory, ctx.values.class)?));
    }
    let trees_remaining = work.max_retained_trees;
    let mut d = ChangeDecoder {
        ctx,
        scan,
        work,
        profile: T::BRANCH.profile(),
        trees_remaining,
        tree_depth: 0,
    };
    let old = before;
    let new = after;
    if old == new
        && let Some(index) = parent
    {
        return Ok(Some(index.share(d.ctx.values.memory, d.ctx.values.class)?));
    }
    let builder = if let Some(index) = parent {
        index.copy_for_changes(d.ctx.values.memory, d.ctx.values.class, &mut |s, b| {
            spend(d.scan, &mut d.ctx.cancelled, s, b)
        })?
    } else {
        Builder::new(d.ctx.values.memory, d.ctx.values.class)?
    };
    let empty = d
        .ctx
        .values
        .memory
        .reserve(512, d.ctx.values.class)?
        .retain(StateRootV1::empty(StateNamespaceV1::Authorization).map_err(|_| invalid())?);
    let mut builder = builder;
    visit_entries(
        old.unwrap_or(&empty),
        new.unwrap_or(&empty),
        &mut d,
        |old, new, d| {
            let Some(cell) = new.or(old) else {
                return Err(invalid());
            };
            let p = d.profile;
            // Full bootstrap includes the container's Metadata leaf. The
            // original column-key reader accepts Entry only; authenticate the
            // metadata with the original presence codec before visiting keys,
            // as the original full typed-container visitor does.
            let TypedCellRefV1::Entry(entry) = cell else {
                return Err(invalid());
            };
            let parsed =
                WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
            if parsed.kind == WorldFieldRecordKindV1::Metadata {
                let expected =
                    WorldFieldKeyV1::new(p.field, WorldFieldRecordKindV1::Metadata, None)
                        .map_err(ValueTreeError::Invalid)?;
                for edge in [old, new].into_iter().flatten() {
                    let TypedCellRefV1::Entry(entry) = edge else {
                        return Err(invalid());
                    };
                    let binding =
                        WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
                    if binding.semantic_key.is_some()
                        || entry.key != expected.as_bytes()
                        || entry.namespace != p.namespace()?
                    {
                        return Err(invalid());
                    }
                    d.ctx.records().presence(&entry.value, &expected, 0)?;
                }
                return d.check();
            }
            let name = key(&p, cell, d)?;
            let id = match &*name {
                ColumnKeyV1::Text(s) if s == "_length" => return Ok(()),
                ColumnKeyV1::Pair(kind, id) if matches!(&**kind,ColumnKeyV1::Text(s)if s=="entry") => {
                    match &**id {
                        ColumnKeyV1::Text(s) => s.as_str(),
                        _ => return Err(invalid()),
                    }
                }
                _ => return Err(invalid()),
            };
            if let Some(new) = new {
                let body = root(&p, new, d)?;
                let TypedCellRefV1::Entry(outer) = new else {
                    return Err(invalid());
                };
                // The owner has already accepted the complete closed receipt.
                // Its resident result metadata may exceed a selected DTO heap;
                // admit that exact output and stream the original Text into it.
                let max_heap = d.work.max_tree_heap;
                let actor = point_typed_column_v1(&p, &body, "actor_id", d, max_heap)?;
                let request = point_typed_column_v1(&p, &body, "request_id", d, max_heap)?;
                let (ColumnCellV1::Text(actor), ColumnCellV1::Text(request)) =
                    (&actor.cell, &request.cell)
                else {
                    return Err(invalid());
                };
                let result = point_typed_entry_v1(&p, &body, "result_id", d)?;
                let parsed = WorldFieldKeyV1::from_bytes(&result.entry.key)
                    .map_err(ValueTreeError::Invalid)?;
                let expected = WorldFieldKeyV1::new(
                    p.field,
                    WorldFieldRecordKindV1::Entry,
                    parsed.semantic_key,
                )
                .map_err(ValueTreeError::Invalid)?;
                let header = read_member_record_header_v1(
                    &result.entry.value,
                    &expected,
                    &mut d.ctx.values,
                    MemberRecordLimitsV1 {
                        semantic_bytes: d.ctx.semantic_bytes,
                        payload_bytes: d.ctx.record_bytes,
                        record_bytes: d.ctx.record_bytes,
                    },
                    &mut d.ctx.cancelled,
                )?;
                if header.codec != MemberPayloadCodecV1::Text
                    || header.semantic_bytes != result.semantic_bytes
                {
                    return Err(invalid());
                }
                let mut pending = builder.prepare(
                    id,
                    actor,
                    request,
                    usize::try_from(header.payload_bytes).map_err(|_| invalid())?,
                    outer.value,
                    &mut |s, b| spend(d.scan, &mut d.ctx.cancelled, s, b),
                )?;
                let mut output = std::io::Cursor::new(pending.result_bytes());
                let decoded = read_record_value_v1(
                    &result.entry.value,
                    &(expected, result.semantic_bytes),
                    &mut d.ctx.records(),
                    header.payload_bytes,
                    &mut output,
                    RecordReadProfileV1 {
                        codec: MemberPayloadCodecV1::Text,
                        nullable: false,
                        integer_max: u64::MAX,
                        labels: RecordReadLabelsV1 {
                            utf8: "request result UTF8",
                            ledger_utf8: "request result ledger",
                            ledger_frame: "request result ledger",
                            boolean: "request result bool",
                            fixed: "request result fixed",
                            profile: "request result codec",
                            integer: "request result integer",
                        },
                    },
                )?;
                if decoded
                    != (RecordValueV1::Text {
                        utf8_bytes: header.payload_bytes,
                    })
                    || output.position() != header.payload_bytes
                {
                    return Err(invalid());
                }
                d.check()?;
                builder.put(pending, &mut |s, b| {
                    spend(d.scan, &mut d.ctx.cancelled, s, b)
                })?;
            } else {
                builder.remove(id, &mut |s, b| spend(d.scan, &mut d.ctx.cancelled, s, b))?;
            }
            Ok(())
        },
    )?;
    Ok(Some(builder.finish()?))
}
