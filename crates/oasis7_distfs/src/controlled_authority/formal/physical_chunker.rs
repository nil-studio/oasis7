/// Frozen chunker v1: deterministic gear rolling state, min 4KiB, mask 14
/// bits, max 64KiB. Boundaries depend on bytes, never filesystem layout.
pub(super) fn chunks(bytes: &[u8]) -> Vec<&[u8]> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut state = 0u64;
    for (index, byte) in bytes.iter().enumerate() {
        let value = (*byte as u64 + 1).wrapping_mul(0x9e3779b185ebca87);
        state = state
            .rotate_left(1)
            .wrapping_add(value ^ value.rotate_left(23));
        let size = index + 1 - start;
        if size >= 4096 && (state & 16383 == 0 || size == super::FRAME) {
            result.push(&bytes[start..=index]);
            start = index + 1;
            state = 0;
        }
    }
    if start < bytes.len() {
        result.push(&bytes[start..]);
    }
    result
}
