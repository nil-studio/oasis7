use super::*;
pub fn inspect_diplomacy_root_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: DiplomacyFieldV1,
    id: &str,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<Option<ReservedValue<StateRootV1>>, ValueTreeError> {
    let key = ctx.key(field.descriptor()?, WorldFieldRecordKindV1::Entry, id)?;
    let Some(value) = ctx.lookup(&key.0)? else {
        ctx.check()?;
        return Ok(None);
    };
    decode(
        field,
        &key,
        value,
        &mut ctx.values,
        ctx.record_bytes,
        &mut ctx.cancelled,
    )
    .map(Some)
}
pub(super) fn decode<R: ValueNodeReader, C: FnMut() -> bool>(
    field: DiplomacyFieldV1,
    key: &(WorldFieldKeyV1, u64),
    value: [u8; 32],
    values: &mut oasis7_formal_protocol::incremental_value::ValueReadContext<'_, R>,
    record_bytes: u64,
    cancel: &mut C,
) -> Result<ReservedValue<StateRootV1>, ValueTreeError> {
    let retained = values.memory.reserve(512, values.class)?;
    let _scratch = values
        .memory
        .reserve(4096 + RECORD_ROOT_BYTES, values.class)?;
    let mut bytes = [0; RECORD_ROOT_BYTES];
    read_member_record_profile_v1(
        &value,
        &key.0,
        values,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::RecordRoot,
            semantic_bytes: key.1,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: key.1,
                payload_bytes: RECORD_ROOT_BYTES as u64,
                record_bytes,
            },
        },
        &mut std::io::sink(),
        &mut Cursor::new(bytes.as_mut_slice()),
        &mut *cancel,
    )?;
    let reference = RecordRootRefV1::from_canonical_bytes(&bytes)
        .map_err(|_| ValueTreeError::Invalid("diplomacy root reference"))?;
    let digest = WorldFieldKeyV1::from_bytes(key.0.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or(ValueTreeError::Invalid("diplomacy owner digest"))?;
    let root = reference
        .validate_binding(&field.schema(), &digest, StateNamespaceV1::Business)
        .map_err(|_| ValueTreeError::Invalid("diplomacy root binding"))?;
    if cancel() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(retained.retain(root.clone()))
}
pub fn write_diplomacy_root_reference_v1<
    W: oasis7_formal_protocol::incremental_value::ValueNodeWriter,
    C: FnMut() -> bool,
>(
    field: DiplomacyFieldV1,
    id: &str,
    root: &StateRootV1,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    if root.namespace != StateNamespaceV1::Business {
        return Err(ValueTreeError::Invalid("diplomacy root namespace").into());
    }
    let _scratch = ctx
        .memory
        .reserve(4096, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let semantic = hash_semantic_key_v1(
        id,
        ctx.memory,
        MemoryClass::Ordinary,
        ctx.key_work,
        &mut ctx.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    let reference = RecordRootRefV1::new(field.schema(), semantic.digest, root.clone())
        .map_err(|_| ValueTreeError::Invalid("diplomacy root child"))?;
    write_member_record_v1(
        field.descriptor()?,
        WorldFieldRecordKindV1::Entry,
        id,
        MemberPayloadRefV1::RecordRoot(&reference),
        ctx,
    )
}
