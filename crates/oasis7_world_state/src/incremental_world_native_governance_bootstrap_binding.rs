//! The original rotation map has an exact ID relation, not an ID normalization.
use super::*;
pub(super) fn source(
    key: &String,
    value: &crate::governance::BootstrapFinalityKeyRotationRecordV1,
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
    let profile = GovernanceBootstrapFinalityKeyRotationsFieldV1::profile()?;
    let recovered: RecoveredTypedRecordV1<
        crate::incremental_world_native::WorldNativeRecordV1<
            crate::governance::BootstrapFinalityKeyRotationRecordV1,
        >,
    > = recover_typed_record_body_v1(body, &profile, ctx, scan, work)?;
    // The exact original codec invokes BootstrapRecord.validate. Qualification
    // therefore checks all original structural bindings as well as outer ID.
    let value = recovered.value().value();
    if value.body().map_err(|_| invalid())?.rotation_id.as_str() != key {
        return Err(invalid());
    }
    ctx.check()?;
    Ok(())
}
