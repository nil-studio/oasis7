//! Authenticated scalar/header diffs skip unchanged manifest and patch graphs.
use super::header;
use super::*;
use crate::{
    incremental_field_member::MemberPayloadCodecV1,
    incremental_record_columns_read::{
        ColumnKeyV1 as Key, ColumnReadProfileV1, read_column_key_v1,
    },
    incremental_record_diff::{self as diff, RecordDiffLabelsV1},
    incremental_record_value_read::RecordValueV1,
};
pub(super) fn advance_body<M, R, E, C>(
    before: &StateRootV1,
    after: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut RuntimeProposalReadWorkV1,
) -> Result<(), ValueTreeError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut full = false;
    let mut status = false;
    let mut options = 0u8;
    let mut cells = work.cells_remaining;
    diff::visit(
        before,
        after,
        &mut ctx.records(),
        &mut cells,
        RecordDiffLabelsV1 {
            invalid: "proposal changed paths",
            cells: "proposal diff cells",
            stack: "proposal diff stack",
            tree: genesis::LABELS,
        },
        |old, new, records| {
            let entry = new.or(old).ok_or_else(invalid)?;
            let parsed =
                WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
            if parsed.kind == WorldFieldRecordKindV1::Metadata {
                if old.is_none() || new.is_none() || old.map(|e| e.value) != new.map(|e| e.value) {
                    return Err(invalid());
                }
                return Ok(());
            }
            let key = read_column_key_v1(
                ColumnReadProfileV1 {
                    field: columns(),
                    schema: schema(),
                    groups: &[],
                    reference_schema: Some(recovery::reference_schema),
                },
                entry,
                records,
                16 << 10,
            )?;
            match &*key {
                Key::Text(name) if header::STATUS_COLUMNS.contains(&name.as_str()) => {
                    status = true;
                }
                Key::Text(name)
                    if matches!(
                        name.as_str(),
                        "id" | "timelock_ticks"
                            | "manifest.version"
                            | "author"
                            | "base_manifest_hash"
                    ) =>
                {
                    let Some(new) = new else {
                        return Err(invalid());
                    };
                    if old.is_none() {
                        return Err(invalid());
                    }
                    let (key, len) =
                        records.key(columns(), WorldFieldRecordKindV1::Entry, &name.as_str())?;
                    let codec = if matches!(name.as_str(), "author" | "base_manifest_hash") {
                        MemberPayloadCodecV1::Text
                    } else {
                        MemberPayloadCodecV1::U64
                    };
                    let result = header::value(
                        &new.value,
                        &(key, len),
                        codec,
                        records,
                        work,
                        &mut std::io::sink(),
                    )?;
                    if !matches!(
                        (codec, result),
                        (MemberPayloadCodecV1::Text, RecordValueV1::Text { .. })
                            | (MemberPayloadCodecV1::U64, RecordValueV1::Unsigned(_))
                    ) {
                        return Err(invalid());
                    }
                }
                Key::Pair(a, b)
                    if matches!(a.as_ref(),Key::Text(name)if matches!(name.as_str(),"queued_at_tick"|"not_before_tick"|"activate_epoch"))
                        && matches!(b.as_ref(),Key::Text(part)if matches!(part.as_str(),"present"|"value")) =>
                {
                    let Key::Text(name) = a.as_ref() else {
                        return Err(invalid());
                    };
                    let index = match name.as_str() {
                        "queued_at_tick" => 0,
                        "not_before_tick" => 1,
                        "activate_epoch" => 2,
                        _ => return Err(invalid()),
                    };
                    options |= 1 << index;
                }
                _ => full = true,
            }
            Ok(())
        },
    )?;
    work.cells_remaining = cells;
    if full {
        stream::qualify(after, ctx, scan, work)?;
    } else {
        if status {
            header::validate_status(after, None, &mut ctx.records(), work)?;
        }
        for (index, name) in ["queued_at_tick", "not_before_tick", "activate_epoch"]
            .into_iter()
            .enumerate()
        {
            if options & (1 << index) != 0 {
                header::validate_optional(after, name, &mut ctx.records(), work)?;
            }
        }
    }
    ctx.check()?;
    Ok(())
}
