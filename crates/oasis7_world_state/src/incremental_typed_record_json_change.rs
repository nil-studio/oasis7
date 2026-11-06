//! Changed JSON validation inherits only an already authenticated typed parent.
//! No old graph hydration or full JSON Value allocation is performed.
use crate::incremental_typed_record_codec::*;
use crate::incremental_typed_record_diff::{ChangeDecoder, TypedCellRefV1};
pub(super) fn accept<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    profile: &TypedRecordProfileV1,
    before: Option<&TypedCellRefV1<'_>>,
    after: Option<&TypedCellRefV1<'_>>,
    d: &mut ChangeDecoder<'_, '_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    d.check()?;
    let Some(after) = after else {
        return Ok(());
    };
    d.trees_remaining = d
        .trees_remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("typed JSON changed trees"))?;
    let after = crate::incremental_typed_record_diff::root(profile, after, d)?;
    let json_profile = crate::incremental_json_records::JsonRecordProfileV1 {
        field: profile.field,
        schema: profile.schema,
    };
    let Some(before) = before else {
        crate::incremental_json_records::qualify_json_subtree_records_v1(
            json_profile,
            &after,
            &mut d.ctx.records(),
            &mut d.work.json,
        )?;
        return Ok(());
    };
    let before = crate::incremental_typed_record_diff::root(profile, before, d)?;
    if *before == *after {
        return Ok(());
    }
    let json = std::mem::replace(
        &mut d.work.json,
        crate::incremental_json_records::JsonReadWorkV1 {
            cells_remaining: 0,
            json_nodes_remaining: 0,
            max_depth: 0,
            max_payload_bytes: 0,
        },
    );
    let initial_cells = json.cells_remaining;
    let mut work = crate::incremental_json_records::JsonChangeWorkV1 {
        paths_remaining: json.cells_remaining,
        json,
        max_pending_pairs: d.work.max_retained_trees,
    };
    // Only the enclosing owner receiver can reach this with a qualified before.
    let result = crate::incremental_json_records::advance_from_qualified_root(
        json_profile,
        &before,
        &after,
        &mut d.ctx.records(),
        &mut work,
    );
    let remaining = work
        .json
        .cells_remaining
        .checked_sub(initial_cells - work.paths_remaining);
    work.json.cells_remaining = remaining.unwrap_or(0);
    d.work.json = work.json;
    d.check()?;
    result?;
    if remaining.is_none() {
        return Err(ValueTreeError::ResourceLimited("typed JSON shared cells"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "incremental_typed_record_json_change_tests.rs"]
mod tests;
