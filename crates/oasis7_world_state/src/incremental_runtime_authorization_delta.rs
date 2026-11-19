//! Closed changed-column proof inherits one exact complete owner-qualified body.
use super::*;
use crate::{
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_columns_read::{ColumnCellV1, ColumnReadProfileV1, read_column_row_v1},
    incremental_record_diff::{self as record_diff, RecordDiffLabelsV1},
};
use oasis7_formal_protocol::incremental_state::StateEntryV1;
use stream::{ChangedColumn as K, classify, validate_changed_group};
fn json_change<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: AuthorizationBodyKindV1,
    before: Option<&StateEntryV1>,
    after: Option<&StateEntryV1>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    json: &mut JsonReadWorkV1,
) -> Result<(), ValueTreeError> {
    let Some(after) = after else {
        return Ok(());
    };
    let profile = ColumnReadProfileV1 {
        field: kind.descriptor(),
        schema: kind.schema(),
        groups: &[],
        reference_schema: None,
    };
    let after = read_column_row_v1(profile, after, records, 16 << 10)?;
    let ColumnCellV1::Root(after) = &after.cell else {
        return Err(invalid());
    };
    let jp = crate::incremental_json_records::JsonRecordProfileV1 {
        field: kind.descriptor(),
        schema: kind.schema(),
    };
    let Some(before) = before else {
        crate::incremental_json_records::qualify_json_subtree_records_v1(jp, after, records, json)?;
        return Ok(());
    };
    let before = read_column_row_v1(profile, before, records, 16 << 10)?;
    let ColumnCellV1::Root(before) = &before.cell else {
        return Err(invalid());
    };
    if before == after {
        return Ok(());
    }
    let inner = std::mem::replace(
        json,
        JsonReadWorkV1 {
            cells_remaining: 0,
            json_nodes_remaining: 0,
            max_depth: 0,
            max_payload_bytes: 0,
        },
    );
    let initial = inner.cells_remaining;
    let mut work = crate::incremental_json_records::JsonChangeWorkV1 {
        json: inner,
        paths_remaining: initial,
        max_pending_pairs: 64,
    };
    // The enclosing receiver authenticates the private complete parent, exact
    // RootSet/body and same memory pool before entering this internal bridge.
    let result = crate::incremental_json_records::advance_from_qualified_root(
        jp, before, after, records, &mut work,
    );
    let left = work
        .json
        .cells_remaining
        .checked_sub(initial - work.paths_remaining);
    work.json.cells_remaining = left.unwrap_or(0);
    *json = work.json;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    result?;
    if left.is_none() {
        return Err(ValueTreeError::ResourceLimited("auth JSON shared cells"));
    }
    Ok(())
}
pub(super) fn advance<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    kind: AuthorizationBodyKindV1,
    before: &StateRootV1,
    after: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    json: &mut JsonReadWorkV1,
) -> Result<(), ValueTreeError> {
    ctx.check()?;
    if let Some(error) = (ctx.take_state_error)() {
        return Err(error);
    }
    let initial = json.cells_remaining;
    let mut cells = initial;
    let mut added = 0i128;
    let mut removed = 0i128;
    let mut effects = false;
    let result = record_diff::visit(
        before,
        after,
        &mut ctx.records(),
        &mut cells,
        RecordDiffLabelsV1 {
            invalid: "auth changed columns",
            cells: "auth changed cells",
            stack: "auth changed stack",
            tree: stream::LABELS,
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
            match classify(kind, entry, records)? {
                c @ K::Required(_, _) => {
                    if new.is_none() {
                        return Err(invalid());
                    }
                    validate_changed_group(kind, after, &c, records, json)?;
                }
                c @ (K::Optional(_, _) | K::Subject) => {
                    validate_changed_group(kind, after, &c, records, json)?
                }
                K::Json(name) => {
                    if new.is_none() {
                        return Err(invalid());
                    }
                    let _ = name;
                    json_change(kind, old, new, records, json)?;
                }
                K::Presenter => {
                    validate_changed_group(kind, after, &K::Presenter, records, json)?;
                    let (key, _) = records.key(
                        kind.descriptor(),
                        WorldFieldRecordKindV1::Entry,
                        &"presenter",
                    )?;
                    if key.as_bytes() == entry.key {
                        json_change(kind, old, new, records, json)?;
                    }
                }
                K::Effects => {
                    effects = true;
                }
                K::EffectMember => {
                    effects = true;
                    stream::validate_effect_member(entry, records)?;
                    added += i128::from(new.is_some());
                    removed += i128::from(old.is_some());
                }
            }
            Ok(())
        },
    );
    let left = json.cells_remaining.checked_sub(initial - cells);
    json.cells_remaining = left.unwrap_or(0);
    ctx.check()?;
    if let Some(error) = (ctx.take_state_error)() {
        return Err(error);
    }
    result?;
    if left.is_none() {
        return Err(ValueTreeError::ResourceLimited(
            "auth combined changed cells",
        ));
    }
    if effects {
        let old = stream::effect_count(before, &mut ctx.records(), json)?;
        let new = stream::effect_count(after, &mut ctx.records(), json)?;
        if i128::from(new) != i128::from(old) + added - removed {
            return Err(invalid());
        }
    }
    ctx.check()
}
