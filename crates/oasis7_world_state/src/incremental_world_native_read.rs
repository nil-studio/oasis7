use super::*;
use oasis7_formal_protocol::incremental_record_root::RECORD_ROOT_BYTES;

pub(crate) fn check<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<(), ValueTreeError> {
    ctx.check()?;
    if let Some(error) = (ctx.take_state_error)() {
        return Err(error);
    }
    Ok(())
}
pub(crate) fn read_body_at_entry_v1<
    F: WorldNativeFieldSpecV1,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: &WorldFieldKeyV1,
    value: &[u8; 32],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<ReservedValue<StateRootV1>, ValueTreeError> {
    check(ctx)?;
    let _phase = ctx.values.memory.reserve(4096, ctx.values.class)?;
    let parsed = WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
    let expected = WorldFieldKeyV1::new(
        F::descriptor()?,
        WorldFieldRecordKindV1::Entry,
        parsed.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    if expected != *key || ctx.root.namespace != parsed.namespace {
        return Err(invalid());
    }
    ctx.values
        .verify_closure(value, ctx.record_bytes, &mut ctx.cancelled)?;
    let mut bytes = [0; RECORD_ROOT_BYTES];
    let info = read_member_record_v1(
        value,
        key,
        &mut ctx.values,
        MemberRecordLimitsV1 {
            semantic_bytes: ctx.semantic_bytes,
            payload_bytes: RECORD_ROOT_BYTES as u64,
            record_bytes: ctx.record_bytes,
        },
        &mut std::io::sink(),
        &mut &mut bytes[..],
        &mut ctx.cancelled,
    )?;
    if info.codec != MemberPayloadCodecV1::RecordRoot
        || info.payload_bytes != RECORD_ROOT_BYTES as u64
    {
        return Err(invalid());
    }
    let owner = WorldFieldKeyV1::from_bytes(key.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or_else(invalid)?;
    let profile = F::profile()?;
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes).map_err(|_| invalid())?;
    let child = reference
        .validate_binding(&profile.schema, &owner, profile.namespace()?)
        .map_err(|_| invalid())?;
    let charge = ctx.values.memory.reserve(512, ctx.values.class)?;
    let result = charge.retain(child.clone());
    check(ctx)?;
    Ok(result)
}
pub(crate) fn read_body_v1<
    F: WorldNativeFieldSpecV1,
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    source_key: &F::Key,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<ReservedValue<StateRootV1>>, ValueTreeError> {
    check(ctx)?;
    let _phase = ctx.values.memory.reserve(4096, ctx.values.class)?;
    let field = F::descriptor()?;
    let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let value = ctx.lookup(&metadata)?.ok_or_else(invalid)?;
    ctx.records().presence(&value, &metadata, 0)?;
    let (key, _) = ctx.key(
        field,
        WorldFieldRecordKindV1::Entry,
        &source_key.semantic_key(),
    )?;
    let Some(value) = ctx.lookup(&key)? else {
        return if F::Key::UNIT {
            Err(invalid())
        } else {
            Ok(None)
        };
    };
    read_body_at_entry_v1::<F, _, _, _, _>(&key, &value, ctx).map(Some)
}
pub(crate) fn recover_world_native_record_v1<F: WorldNativeFieldSpecV1>(
    key: &F::Key,
    ctx: &mut WorldFieldReadContextV1<
        '_,
        impl MerkleNodeReader,
        impl ValueNodeReader,
        impl FnMut() -> Option<ValueTreeError>,
        impl FnMut() -> bool,
    >,
    scan: &mut StateScanBudget,
    work: &mut TypedRecordReadWorkV1,
) -> Result<Option<RecoveredTypedRecordV1<WorldNativeRecordV1<F::Value>>>, ValueTreeError> {
    let Some(body) = read_body_v1::<F, _, _, _, _>(key, ctx)? else {
        return Ok(None);
    };
    let profile = F::profile()?;
    let recovered = recover_typed_record_body_v1(&body, &profile, ctx, scan, work)?;
    F::validate_record_binding(key.key_ref(), &body, ctx, scan, work)?;
    check(ctx)?;
    Ok(Some(recovered))
}
