//! Original ordinal collection diffs; no scan of unchanged ordinal entries.
use super::*;
use crate::{
    incremental_record_diff::{self as record_diff, RecordDiffLabelsV1},
    incremental_rollback_records::RollbackPathV1 as P,
};
use oasis7_formal_protocol::incremental_state::StateEntryV1;
// The caller supplies a separate finite traversal budget. Cell validation keeps
// consuming the existing RollbackWork; neither allowance resets in recursion.
pub(super) fn advance_vector<R: ValueNodeReader, C: FnMut() -> bool>(
    before: &StateRootV1,
    after: &StateRootV1,
    prefix: &[P],
    dispositions: bool,
    ctx: &mut RollbackDecodeContextV1<'_, '_, R, C>,
    diff_cells: &mut u64,
) -> Result<(), ValueTreeError> {
    let schema = *blake3::hash(b"runtime.rollback.outcome.v1").as_bytes();
    let old = crate::incremental_rollback_qualification::scope(before, schema, prefix, ctx)?;
    let new = crate::incremental_rollback_qualification::scope(after, schema, prefix, ctx)?;
    if old == new {
        return Ok(());
    }
    let oldcount = stream::vector_count(before, prefix, ctx)?;
    let newcount = stream::vector_count(after, prefix, ctx)?;
    stream::vector_shape(after, prefix, newcount, ctx)?;
    let limits = RollbackDecodeLimitsV1 {
        max_output_bytes: ctx.limits.max_output_bytes,
        max_text_bytes: ctx.limits.max_text_bytes,
        max_record_bytes: ctx.limits.max_record_bytes,
    };
    let work = &mut *ctx.work;
    let mut records = InventoryValueReader {
        values: ctx.values,
        key_work: ctx.keys,
        semantic_bytes: limits.max_record_bytes,
        record_bytes: limits.max_record_bytes,
        cancelled: &mut *ctx.cancelled,
    };
    record_diff::visit(
        &old,
        &new,
        &mut records,
        diff_cells,
        RecordDiffLabelsV1 {
            invalid: "rollback ordinal delta",
            cells: "rollback ordinal traversal",
            stack: "rollback ordinal stack",
            tree: LABELS,
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
            let selected = select(entry, records, dispositions)?;
            match selected {
                Selected::Marker | Selected::Count => {
                    if old.is_none() || new.is_none() {
                        return Err(invalid());
                    }
                    if matches!(selected, Selected::Marker)
                        && old.map(|e| e.value) != new.map(|e| e.value)
                    {
                        return Err(invalid());
                    }
                }
                Selected::Ordinal(i) => {
                    if new.is_some() {
                        if i >= newcount {
                            return Err(invalid());
                        }
                        let mut local = RollbackDecodeContextV1 {
                            values: records.values,
                            keys: records.key_work,
                            work: &mut *work,
                            limits: RollbackDecodeLimitsV1 {
                                max_output_bytes: limits.max_output_bytes,
                                max_text_bytes: limits.max_text_bytes,
                                max_record_bytes: limits.max_record_bytes,
                            },
                            cancelled: &mut *records.cancelled,
                        };
                        if dispositions {
                            stream::validate_disposition_at(before, after, i, &mut local)?;
                        } else {
                            stream::validate_vector_text_at(before, after, prefix, i, &mut local)?;
                        }
                    }
                }
            }
            Ok(())
        },
    )?;
    // A shrinking vector must remove the complete old suffix. Checking only the
    // changed indices and leafcount could otherwise accept an interior hole.
    for i in newcount..oldcount {
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled);
        }
        *diff_cells = diff_cells
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("rollback suffix work"))?;
        let key = records.key(descriptor(), WorldFieldRecordKindV1::Entry, &("ordinal", i))?;
        if incremental_record_tree::lookup(&new, &key.0, &mut records, LABELS)?.is_some() {
            return Err(invalid());
        }
    }
    Ok(())
}
#[derive(Clone, Copy)]
enum Selected {
    Marker,
    Count,
    Ordinal(u64),
}
fn select<R: ValueNodeReader, C: FnMut() -> bool>(
    entry: &StateEntryV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    dispositions: bool,
) -> Result<Selected, ValueTreeError> {
    let parsed = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let key = WorldFieldKeyV1::new(
        descriptor(),
        WorldFieldRecordKindV1::Entry,
        parsed.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != entry.key {
        return Err(invalid());
    }
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: 256,
        payload_bytes: records.record_bytes,
        record_bytes: records.record_bytes,
    };
    let info = read_member_record_header_v1(
        &entry.value,
        &key,
        records.values,
        limits,
        &mut *records.cancelled,
    )?;
    let len = usize::try_from(info.semantic_bytes).map_err(|_| invalid())?;
    records.key_work.bytes_remaining = records
        .key_work
        .bytes_remaining
        .checked_sub(info.semantic_bytes)
        .ok_or(ValueTreeError::ResourceLimited(
            "rollback ordinal key bytes",
        ))?;
    if len > 256 {
        return Err(invalid());
    }
    let mut semantic = [0u8; 256];
    read_member_record_key_v1(
        &entry.value,
        &key,
        records.values,
        limits,
        &mut Cursor::new(&mut semantic[..len]),
        &mut *records.cancelled,
    )?;
    match read_semantic_key_v1(&semantic[..len]).map_err(ValueTreeError::Invalid)? {
        SemanticKeyRefV1::Unit
            if info.codec == MemberPayloadCodecV1::Unit && info.payload_bytes == 0 =>
        {
            Ok(Selected::Marker)
        }
        SemanticKeyRefV1::Pair { left, right } => match (
            left.decode().map_err(ValueTreeError::Invalid)?,
            right.decode().map_err(ValueTreeError::Invalid)?,
        ) {
            (SemanticKeyRefV1::Text("column"), SemanticKeyRefV1::Text("count"))
                if info.codec == MemberPayloadCodecV1::U64 && info.payload_bytes == 8 =>
            {
                Ok(Selected::Count)
            }
            (SemanticKeyRefV1::Text("ordinal"), SemanticKeyRefV1::Unsigned(i))
                if (dispositions
                    && info.codec == MemberPayloadCodecV1::RecordRoot
                    && info.payload_bytes == RECORD_ROOT_BYTES as u64)
                    || (!dispositions && info.codec == MemberPayloadCodecV1::Text) =>
            {
                Ok(Selected::Ordinal(i))
            }
            _ => Err(invalid()),
        },
        _ => Err(invalid()),
    }
}
