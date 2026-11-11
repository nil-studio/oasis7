//! Charged canonical original keys; only Unit, original Text or original u64.
use super::*;
use crate::incremental_semantic_key::{SemanticKeyRefV1, read_semantic_key_v1};
use oasis7_formal_protocol::incremental_record_root::RECORD_ROOT_BYTES;
pub(super) fn original_ref<F: WorldNativeFieldSpecV1>(
    bytes: &[u8],
) -> Result<WorldNativeKeyRefV1<'_>, ValueTreeError> {
    match read_semantic_key_v1(bytes).map_err(ValueTreeError::Invalid)? {
        SemanticKeyRefV1::Unit if F::Key::UNIT => Ok(WorldNativeKeyRefV1::Unit),
        SemanticKeyRefV1::Text(text)
            if !F::Key::UNIT && F::Key::accepts_column(&ColumnKeyV1::Text(String::new())) =>
        {
            Ok(WorldNativeKeyRefV1::Text(text))
        }
        SemanticKeyRefV1::Unsigned(n)
            if !F::Key::UNIT && F::Key::accepts_column(&ColumnKeyV1::Ordinal(n)) =>
        {
            Ok(WorldNativeKeyRefV1::Unsigned)
        }
        _ => Err(invalid()),
    }
}
pub(super) fn canonical<F: WorldNativeFieldSpecV1>(
    entry: &StateEntryV1,
) -> Result<WorldFieldKeyV1, ValueTreeError> {
    let parsed = WorldFieldKeyV1::from_bytes(&entry.key).map_err(ValueTreeError::Invalid)?;
    let descriptor = F::descriptor()?;
    let expected = WorldFieldKeyV1::new(descriptor, parsed.kind, parsed.semantic_key)
        .map_err(ValueTreeError::Invalid)?;
    if expected.as_bytes() != entry.key
        || descriptor.destination != FieldDestination::State(entry.namespace)
        || parsed.namespace != entry.namespace
    {
        return Err(invalid());
    }
    Ok(expected)
}
pub(super) fn original<F: WorldNativeFieldSpecV1>(
    entry: &StateEntryV1,
    ctx: &mut WorldFieldReadContextV1<
        '_,
        impl MerkleNodeReader,
        impl ValueNodeReader,
        impl FnMut() -> Option<ValueTreeError>,
        impl FnMut() -> bool,
    >,
    max_key_heap: usize,
) -> Result<ReservedValue<Vec<u8>>, ValueTreeError> {
    read::check(ctx)?;
    let key = canonical::<F>(entry)?;
    let parsed = WorldFieldKeyV1::from_bytes(key.as_bytes()).map_err(ValueTreeError::Invalid)?;
    if parsed.kind != WorldFieldRecordKindV1::Entry || parsed.semantic_key.is_none() {
        return Err(invalid());
    }
    let limits = MemberRecordLimitsV1 {
        semantic_bytes: ctx.semantic_bytes,
        payload_bytes: RECORD_ROOT_BYTES as u64,
        record_bytes: ctx.record_bytes,
    };
    let info = read_member_record_header_v1(
        &entry.value,
        &key,
        &mut ctx.values,
        limits,
        &mut ctx.cancelled,
    )?;
    if info.codec != MemberPayloadCodecV1::RecordRoot
        || info.payload_bytes != RECORD_ROOT_BYTES as u64
    {
        return Err(invalid());
    }
    let length = usize::try_from(info.semantic_bytes).map_err(|_| invalid())?;
    let bytes = length
        .checked_add(512)
        .ok_or(ValueTreeError::ResourceLimited("native key heap"))?;
    if bytes > max_key_heap {
        return Err(ValueTreeError::ResourceLimited("native key heap"));
    }
    let charge = ctx.values.memory.reserve(bytes, ctx.values.class)?;
    let mut semantic = vec![0; length];
    let result = crate::incremental_field_member::read_member_record_key_v1(
        &entry.value,
        &key,
        &mut ctx.values,
        limits,
        &mut std::io::Cursor::new(semantic.as_mut_slice()),
        &mut ctx.cancelled,
    );
    read::check(ctx)?;
    result?;
    original_ref::<F>(&semantic)?;
    let output = charge.retain(semantic);
    read::check(ctx)?;
    Ok(output)
}
