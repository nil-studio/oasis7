//! The original rotation map has an exact ID relation, not an ID normalization.
use super::*;
pub(super) fn source(
    key: &String,
    value: &crate::governance::FinalityKeyRotationRecordV1,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    class: oasis7_proto::memory_budget::MemoryClass,
) -> Result<(), ValueTreeError> {
    let _phase = memory.reserve(128 << 10, class)?;
    if key.is_empty() || key.len() > 256 || key.trim() != key {
        return Err(invalid());
    }
    value.validate().map_err(|_| invalid())?;
    if value.body().map_err(|_| invalid())?.rotation_id != *key {
        return Err(invalid());
    }
    Ok(())
}
pub(super) fn persisted<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: WorldNativeKeyRefV1<'_>,
    body: &StateRootV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
) -> Result<(), ValueTreeError> {
    let WorldNativeKeyRefV1::Text(key) = key else {
        return Err(invalid());
    };
    if key.is_empty() || key.len() > 256 || key.trim() != key {
        return Err(invalid());
    }
    let _phase = ctx.values.memory.reserve(8192, ctx.values.class)?;
    let mut current = ctx
        .values
        .memory
        .reserve(512, ctx.values.class)?
        .retain(body.clone());
    let profile = GovernanceFinalityKeyRotationsFieldV1::profile()?;
    let trees_remaining = work.max_retained_trees;
    let mut d = crate::incremental_typed_record_diff::ChangeDecoder {
        ctx,
        scan,
        work,
        profile,
        trees_remaining,
        tree_depth: 0,
    };
    let path = [
        "value",
        "authorized_action",
        "action",
        "body",
        "rotation_id",
    ];
    for (index, name) in path.iter().enumerate() {
        let row = crate::incremental_typed_record_diff::point_typed_column_v1(
            &profile, &current, name, &mut d, 4096,
        )?;
        if index + 1 == path.len() {
            let matches = matches!(&row.cell,ColumnCellV1::Text(id) if id==key && id.len()<=256);
            d.check()?;
            return if matches { Ok(()) } else { Err(invalid()) };
        }
        let ColumnCellV1::Root(root) = &row.cell else {
            return Err(invalid());
        };
        current = d
            .ctx
            .values
            .memory
            .reserve(512, d.ctx.values.class)?
            .retain(root.clone());
    }
    Err(invalid())
}
