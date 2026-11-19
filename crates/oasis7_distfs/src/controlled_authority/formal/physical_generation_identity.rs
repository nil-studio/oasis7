//! One uncached current-generation observation under an actual namespace pin.
//! Identity is not a completed coverage/retirement proof. A changed identity
//! invalidates local fast recovery until the closure is fully rebuilt.
use super::*;
pub(crate) fn current_generation_digest_uncached(
    root: &Path,
    memory: &oasis7_proto::memory_budget::MemoryBudget,
    class: oasis7_proto::memory_budget::MemoryClass,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<Option<[u8; 32]>, FormalError> {
    if cancelled() {
        return Err(invalid("generation identity cancelled"));
    }
    // Same two 65536-entry/16MiB metadata bounds as the admitted native reader.
    // All decoded metadata is local and dropped before this reservation ends.
    let _phase = memory
        .reserve(128 * 1024 * 1024, class)
        .map_err(|_| invalid("generation identity metadata admission"))?;
    if !enabled(root)? {
        return Err(invalid("generation identity unknown physical capability"));
    }
    let _pin = pin(root, false)?
        .ok_or_else(|| invalid("generation identity requires actual physical pin"))?;
    let current = current(root)?;
    let digest = match current {
        None => None,
        Some(id) => {
            let _manifest = manifest(root, &id)?;
            let mut digest = [0; 32];
            hex::decode_to_slice(&id, &mut digest)
                .map_err(|_| invalid("generation identity digest"))?;
            Some(digest)
        }
    };
    if cancelled() {
        return Err(invalid("generation identity final cancellation"));
    }
    Ok(digest)
}
