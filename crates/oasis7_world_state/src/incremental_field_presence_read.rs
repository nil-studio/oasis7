use super::*;
use crate::{
    incremental_key_output::KeyOutput, incremental_world_field_key::WORLD_FIELD_KEY_MAX_BYTES,
};
use oasis7_formal_protocol::incremental_value::{ValueNodeReader, ValueReadContext};

/// Raw framing/key-digest authentication only. Output is provisional until
/// success; the registered owner still validates semantic tokens and identity.
pub fn read_field_presence_v1<R: ValueNodeReader>(
    expected_root: &[u8; 32],
    expected_key: &WorldFieldKeyV1,
    context: &mut ValueReadContext<'_, R>,
    limits: PresenceRecordLimitsV1,
    key_output: &mut impl Write,
    mut cancelled: impl FnMut() -> bool,
) -> Result<PresenceRecordInfoV1, ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let binding =
        WorldFieldKeyV1::from_bytes(expected_key.as_bytes()).map_err(ValueTreeError::Invalid)?;
    if !matches!(
        binding.kind,
        WorldFieldRecordKindV1::Metadata | WorldFieldRecordKindV1::Group
    ) {
        return Err(ValueTreeError::Invalid("presence expected kind"));
    }
    let _metadata = context.memory.reserve(4096, context.class)?;
    let root = context.load_root(expected_root)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    if root.0.logical_len < FIELD_PRESENCE_HEADER_BYTES as u64 {
        return Err(ValueTreeError::Invalid("presence header"));
    }
    let mut header = [0; FIELD_PRESENCE_HEADER_BYTES];
    context.read_range(
        &root,
        0,
        header.len() as u64,
        &mut &mut header[..],
        &mut cancelled,
    )?;
    let p = FIELD_PRESENCE_DOMAIN.len();
    if &header[..p] != FIELD_PRESENCE_DOMAIN {
        return Err(ValueTreeError::Invalid("presence domain"));
    }
    let field_bytes = usize::from(u16::from_be_bytes(header[p..p + 2].try_into().unwrap()));
    if field_bytes > WORLD_FIELD_KEY_MAX_BYTES || field_bytes != expected_key.as_bytes().len() {
        return Err(ValueTreeError::Invalid("presence field length"));
    }
    let info = PresenceRecordInfoV1 {
        kind: binding.kind,
        semantic_bytes: u64::from_be_bytes(header[p + 2..].try_into().unwrap()),
    };
    let total = info.total(field_bytes)?;
    if total != root.0.logical_len {
        return Err(ValueTreeError::Invalid("presence record length"));
    }
    info.admit(total, limits)?;
    let mut field = [0; WORLD_FIELD_KEY_MAX_BYTES];
    context.read_range(
        &root,
        FIELD_PRESENCE_HEADER_BYTES as u64,
        field_bytes as u64,
        &mut &mut field[..field_bytes],
        &mut cancelled,
    )?;
    if &field[..field_bytes] != expected_key.as_bytes() {
        return Err(ValueTreeError::Invalid("presence expected field"));
    }
    if let Some(digest) = binding.semantic_key {
        let mut checked = KeyOutput {
            output: key_output,
            hash: blake3::Hasher::new(),
        };
        context.read_range(
            &root,
            (FIELD_PRESENCE_HEADER_BYTES + field_bytes) as u64,
            info.semantic_bytes,
            &mut checked,
            &mut cancelled,
        )?;
        if checked.hash.finalize().as_bytes() != &digest {
            return Err(ValueTreeError::Invalid("presence semantic digest"));
        }
    }
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(info)
}
