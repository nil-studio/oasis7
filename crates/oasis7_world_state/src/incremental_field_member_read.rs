use super::*;
use crate::incremental_world_field_key::WORLD_FIELD_KEY_MAX_BYTES;
use oasis7_formal_protocol::incremental_value::{ValueNodeReader, ValueReadContext};

use crate::incremental_key_output::KeyOutput;

/// Authenticates framing and original-key digest, then returns raw key/payload
/// ranges. This does NOT decode semantic tokens, UTF-8 or payload values.
/// Both output sinks remain provisional until success; their storage is caller
/// admitted. `expected_key` must come from the applicable registered owner.
pub fn read_member_record_v1<R: ValueNodeReader>(
    expected_root: &[u8; 32],
    expected_key: &WorldFieldKeyV1,
    context: &mut ValueReadContext<'_, R>,
    limits: MemberRecordLimitsV1,
    key_output: &mut impl Write,
    payload_output: &mut impl Write,
    cancelled: impl FnMut() -> bool,
) -> Result<MemberRecordInfoV1, ValueTreeError> {
    read_record(
        expected_root,
        expected_key,
        context,
        (limits, None),
        key_output,
        payload_output,
        cancelled,
    )
}
/// Additionally checks the expected declared codec/key length before byte
/// admission or output. This does not decode a Text/Ledger payload itself.
pub fn read_member_record_profile_v1<R: ValueNodeReader>(
    expected_root: &[u8; 32],
    expected_key: &WorldFieldKeyV1,
    context: &mut ValueReadContext<'_, R>,
    profile: MemberReadProfileV1,
    key_output: &mut impl Write,
    payload_output: &mut impl Write,
    cancelled: impl FnMut() -> bool,
) -> Result<MemberRecordInfoV1, ValueTreeError> {
    read_record(
        expected_root,
        expected_key,
        context,
        (
            profile.limits,
            Some((profile.codec, profile.semantic_bytes)),
        ),
        key_output,
        payload_output,
        cancelled,
    )
}
fn read_record<R: ValueNodeReader>(
    expected_root: &[u8; 32],
    expected_key: &WorldFieldKeyV1,
    context: &mut ValueReadContext<'_, R>,
    admission: (MemberRecordLimitsV1, Option<(MemberPayloadCodecV1, u64)>),
    key_output: &mut impl Write,
    payload_output: &mut impl Write,
    mut cancelled: impl FnMut() -> bool,
) -> Result<MemberRecordInfoV1, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let _metadata = context.memory.reserve(4096, context.class)?;
    let root = context.load_root(expected_root)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if root.0.logical_len < FIELD_MEMBER_HEADER_BYTES as u64 {
        return Err(ValueTreeError::Invalid("member header"));
    }
    let mut header = [0; FIELD_MEMBER_HEADER_BYTES];
    context.read_range(
        &root,
        0,
        header.len() as u64,
        &mut &mut header[..],
        &mut cancelled,
    )?;
    let p = FIELD_MEMBER_DOMAIN.len();
    if &header[..p] != FIELD_MEMBER_DOMAIN {
        return Err(ValueTreeError::Invalid("member domain"));
    }
    let field_bytes = usize::from(u16::from_be_bytes(header[p..p + 2].try_into().unwrap()));
    if field_bytes > WORLD_FIELD_KEY_MAX_BYTES || field_bytes != expected_key.as_bytes().len() {
        return Err(ValueTreeError::Invalid("member field length"));
    }
    let info = MemberRecordInfoV1 {
        codec: MemberPayloadCodecV1::parse(header[p + 2]).map_err(ValueTreeError::Invalid)?,
        semantic_bytes: u64::from_be_bytes(header[p + 3..p + 11].try_into().unwrap()),
        payload_bytes: u64::from_be_bytes(header[p + 11..].try_into().unwrap()),
    };
    let total = info.total(field_bytes)?;
    if total != root.0.logical_len {
        return Err(ValueTreeError::Invalid("member record length"));
    }
    if admission
        .1
        .is_some_and(|(codec, bytes)| info.codec != codec || info.semantic_bytes != bytes)
    {
        return Err(ValueTreeError::Invalid("member expected profile"));
    }
    let binding =
        WorldFieldKeyV1::from_bytes(expected_key.as_bytes()).map_err(ValueTreeError::Invalid)?;
    info.codec
        .framing(binding.kind, info.payload_bytes)
        .map_err(ValueTreeError::Invalid)?;
    info.admit(total, admission.0)?;
    let mut field = [0; WORLD_FIELD_KEY_MAX_BYTES];
    context.read_range(
        &root,
        FIELD_MEMBER_HEADER_BYTES as u64,
        field_bytes as u64,
        &mut &mut field[..field_bytes],
        &mut cancelled,
    )?;
    if &field[..field_bytes] != expected_key.as_bytes() {
        return Err(ValueTreeError::Invalid("member expected field"));
    }
    let digest = binding
        .semantic_key
        .ok_or(ValueTreeError::Invalid("member missing semantic digest"))?;
    let start = (FIELD_MEMBER_HEADER_BYTES + field_bytes) as u64;
    let mut checked = KeyOutput {
        output: key_output,
        hash: blake3::Hasher::new(),
    };
    context.read_range(
        &root,
        start,
        info.semantic_bytes,
        &mut checked,
        &mut cancelled,
    )?;
    if checked.hash.finalize().as_bytes() != &digest {
        return Err(ValueTreeError::Invalid("member semantic digest"));
    }
    context.read_range(
        &root,
        start + info.semantic_bytes,
        info.payload_bytes,
        payload_output,
        &mut cancelled,
    )?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(info)
}
