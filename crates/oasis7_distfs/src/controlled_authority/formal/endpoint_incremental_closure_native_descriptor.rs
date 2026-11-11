//! Retain the exact already-authenticated canonical descriptor original.
use super::*;
/// Only the cold material owner calls this after original canonical typed
/// decoding and native/artifact verification; it is not a qualification ctor.
pub(super) fn retain_native_descriptor_v1(
    bytes: &[u8],
    original: &[u8; 32],
    memory: &MemoryBudget,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<ReservedValue<Vec<u8>>, FormalError> {
    if cancelled()
        || bytes.is_empty()
        || bytes.len() > oasis7_formal_protocol::native_environment::MAX_NATIVE_DESCRIPTOR_BYTES
    {
        return Err(invalid("native worker descriptor limit/cancellation"));
    }
    let fee = memory
        .reserve(
            bytes
                .len()
                .checked_add(std::mem::size_of::<Vec<u8>>() + 256)
                .ok_or_else(|| invalid("native descriptor admission overflow"))?,
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("native descriptor original admission"))?;
    let mut hash = blake3::Hasher::new();
    let mut retained = Vec::with_capacity(bytes.len());
    if retained.capacity() != bytes.len() {
        return Err(invalid("native descriptor retained capacity"));
    }
    for part in bytes.chunks(64 << 10) {
        if cancelled() {
            return Err(invalid("native descriptor retention cancelled"));
        }
        hash.update(part);
        retained.extend_from_slice(part);
    }
    if hash.finalize().as_bytes() != original || cancelled() {
        return Err(invalid("native descriptor original identity"));
    }
    Ok(fee.retain(retained))
}
