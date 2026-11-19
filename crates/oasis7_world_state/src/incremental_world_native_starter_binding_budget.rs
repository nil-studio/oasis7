//! Finite aggregate bridge, charged back to the caller's continuing native
//! record allowance. Runtime Current lookups keep their own continuing work.
use super::starter_binding_index::*;
use super::*;
use crate::incremental_world_native_gameplay::StarterOcClaimsFieldV1 as Field;
fn limited() -> ValueTreeError {
    ValueTreeError::ResourceLimited("starter aggregate index work")
}
fn work(
    native: &WorldNativeQualificationWorkV1,
    rows: usize,
    physical: u64,
    changes: usize,
) -> Result<StarterBindingIndexWorkV1, ValueTreeError> {
    let row_text = native
        .max_key_heap
        .checked_add(
            native
                .typed
                .max_tree_heap
                .checked_mul(3)
                .ok_or_else(limited)?,
        )
        .and_then(|n| n.checked_add(crate::incremental_world_field_key::WORLD_FIELD_KEY_MAX_BYTES))
        .ok_or_else(limited)?;
    let n = physical
        .checked_add(u64::try_from(rows).map_err(|_| limited())?)
        .and_then(|n| n.checked_add(u64::try_from(changes).ok()?))
        .and_then(|n| n.checked_add(1))
        .and_then(|n| n.checked_mul(32))
        .and_then(|n| n.checked_add(256))
        .ok_or_else(limited)?
        .min(native.records_remaining);
    let r = u64::try_from(rows.max(1)).map_err(|_| limited())?;
    let text = u64::try_from(row_text).map_err(|_| limited())?;
    let unit = text
        .checked_mul(2)
        .and_then(|n| {
            n.checked_add(2 * crate::incremental_world_field_key::WORLD_FIELD_KEY_MAX_BYTES as u64)
        })
        .ok_or_else(limited)?;
    Ok(StarterBindingIndexWorkV1 {
        max_rows: rows,
        max_row_text_bytes: row_text,
        max_pending_rows: native.max_pending_records,
        steps_remaining: n,
        bytes_remaining: n.checked_mul(text).ok_or_else(limited)?,
        comparison_bytes_remaining: n
            .checked_mul(r)
            .and_then(|n| n.checked_mul(unit))
            .ok_or_else(limited)?,
        moved_slots_remaining: n
            .checked_mul(r)
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(limited)?,
        pending_entries_remaining: usize::try_from(n).map_err(|_| limited())?,
    })
}
fn debit<T, E: From<ValueTreeError>>(
    native: &mut WorldNativeQualificationWorkV1,
    before: u64,
    remaining: u64,
    result: Result<T, E>,
) -> Result<T, E> {
    let left = before
        .checked_sub(remaining)
        .and_then(|used| native.records_remaining.checked_sub(used));
    native.records_remaining = left.unwrap_or(0);
    match result {
        Err(error) => Err(error),
        Ok(value) => {
            if left.is_none() {
                return Err(limited().into());
            }
            Ok(value)
        }
    }
}
pub(super) fn build(
    field: &ReservedValue<WorldNativeFieldQualificationV1<Field>>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<
        '_,
        impl MerkleNodeReader,
        impl ValueNodeReader,
        impl FnMut() -> Option<ValueTreeError>,
        impl FnMut() -> bool,
    >,
    scan: &mut StateScanBudget,
    native: &mut WorldNativeQualificationWorkV1,
) -> Result<ReservedValue<StarterBindingIndexV1>, ValueTreeError> {
    let rows = usize::try_from(field.records()).map_err(|_| limited())?;
    let ns = Field::profile()?.namespace()?;
    let mut index = work(native, rows, roots.state_root(ns).leaf_count, 0)?;
    let before = index.steps_remaining;
    let result = build_starter_binding_index_v1(field, roots, ctx, scan, native, &mut index);
    debit(native, before, index.steps_remaining, result)
}
pub(super) fn accept(
    parent: &ReservedValue<StarterBindingIndexV1>,
    field: &ReservedValue<WorldNativeFieldQualificationV1<Field>>,
    verified: &WorldNativeVerifiedTransitionV1,
    ctx: &mut WorldFieldReadContextV1<
        '_,
        impl MerkleNodeReader,
        impl ValueNodeReader,
        impl FnMut() -> Option<ValueTreeError>,
        impl FnMut() -> bool,
    >,
    scan: &mut StateScanBudget,
    native: &mut WorldNativeQualificationWorkV1,
) -> Result<ReservedValue<StarterBindingIndexV1>, WorldFieldOverlayErrorV1> {
    let record = verified.record();
    let rows = parent
        .records()
        .checked_add(record.writes.len())
        .ok_or_else(limited)?;
    let ns = Field::profile()?.namespace()?;
    let mut index = work(
        native,
        rows,
        record.after.state_root(ns).leaf_count,
        record.writes.len(),
    )?;
    let before = index.steps_remaining;
    let result = accept_starter_binding_index_verified_v1(
        parent, field, verified, ctx, scan, native, &mut index,
    );
    debit(native, before, index.steps_remaining, result)
}
