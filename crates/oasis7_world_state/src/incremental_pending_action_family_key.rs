//! Fixed-stack position extraction from authenticated original row semantics.
use crate::{
    incremental_runtime_pending_action_record::PendingActionPositionV1,
    incremental_runtime_pending_actions::PENDING_ACTIONS_FIELD_V1,
    incremental_semantic_key::{SemanticKeyRefV1, read_semantic_key_v1},
    incremental_typed_record_codec::*,
};
pub(super) fn identify(
    namespace: StateNamespaceV1,
    bytes: &[u8],
) -> Result<Option<WorldFieldRecordKindV1>, ValueTreeError> {
    if !bytes.starts_with(b"oasis7.iws.world-field-key.v1\0") {
        return Ok(None);
    }
    let binding = WorldFieldKeyV1::from_bytes(bytes).map_err(ValueTreeError::Invalid)?;
    if binding.owner != PENDING_ACTIONS_FIELD_V1.owner
        || binding.name != PENDING_ACTIONS_FIELD_V1.name
    {
        return Ok(None);
    }
    let expected =
        WorldFieldKeyV1::new(PENDING_ACTIONS_FIELD_V1, binding.kind, binding.semantic_key)
            .map_err(ValueTreeError::Invalid)?;
    if namespace != StateNamespaceV1::Scheduler || expected.as_bytes() != bytes {
        return Err(invalid());
    }
    match binding.kind {
        WorldFieldRecordKindV1::Metadata | WorldFieldRecordKindV1::Entry => Ok(Some(binding.kind)),
        _ => Err(invalid()),
    }
}
pub(super) fn position<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key_bytes: &[u8],
    value: &[u8; 32],
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<PendingActionPositionV1, ValueTreeError> {
    let binding = WorldFieldKeyV1::from_bytes(key_bytes).map_err(ValueTreeError::Invalid)?;
    let key = WorldFieldKeyV1::new(
        PENDING_ACTIONS_FIELD_V1,
        WorldFieldRecordKindV1::Entry,
        binding.semantic_key,
    )
    .map_err(ValueTreeError::Invalid)?;
    if key.as_bytes() != key_bytes {
        return Err(invalid());
    }
    let mut semantic = [0u8; 128];
    let info = read_member_record_v1(
        value,
        &key,
        &mut ctx.values,
        MemberRecordLimitsV1 {
            semantic_bytes: 128,
            payload_bytes: oasis7_formal_protocol::incremental_record_root::RECORD_ROOT_BYTES
                as u64,
            record_bytes: ctx.record_bytes,
        },
        &mut &mut semantic[..],
        &mut std::io::sink(),
        &mut ctx.cancelled,
    )?;
    if info.codec != MemberPayloadCodecV1::RecordRoot {
        return Err(invalid());
    }
    let n = usize::try_from(info.semantic_bytes).map_err(|_| invalid())?;
    let SemanticKeyRefV1::Pair { left, right } =
        read_semantic_key_v1(semantic.get(..n).ok_or_else(invalid)?)
            .map_err(ValueTreeError::Invalid)?
    else {
        return Err(invalid());
    };
    let (SemanticKeyRefV1::Unsigned(era), SemanticKeyRefV1::Unsigned(ordinal)) = (
        left.decode().map_err(ValueTreeError::Invalid)?,
        right.decode().map_err(ValueTreeError::Invalid)?,
    ) else {
        return Err(invalid());
    };
    Ok(PendingActionPositionV1 { era, ordinal })
}
