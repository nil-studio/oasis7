//! Charged canonical path keys shared by fixed owner operations.
//! Callers run this preparation inside their transaction poison scope.
use crate::{
    incremental_field_member::*,
    incremental_semantic_key::{SemanticKeyV1, hash_semantic_key_v1, write_semantic_key_v1},
};
use oasis7_formal_protocol::{
    incremental_state::StatePageWriter, incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::io::Write;
pub(crate) fn prepare_semantic_path_key_v1<
    K: SemanticKeyV1 + ?Sized,
    W: StatePageWriter,
    C: FnMut() -> bool,
>(
    key: &K,
    write: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<ReservedValue<Vec<u8>>, MemberRecordError> {
    let size = hash_semantic_key_v1(
        key,
        write.memory,
        MemoryClass::Ordinary,
        write.key_work,
        &mut write.cancelled,
    )
    .map_err(MemberRecordError::Key)?
    .encoded_bytes;
    if size > write.limits.semantic_bytes {
        return Err(ValueTreeError::ResourceLimited("fixed owner path key").into());
    }
    let n = usize::try_from(size).map_err(|_| ValueTreeError::Invalid("fixed path length"))?;
    let charge = write
        .memory
        .reserve(
            n.checked_add(128)
                .ok_or(ValueTreeError::Invalid("fixed path length"))?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut bytes = vec![0; n];
    if bytes.capacity() > charge.bytes().saturating_sub(128) {
        return Err(ValueTreeError::Invalid("fixed path capacity").into());
    }
    let mut cursor = std::io::Cursor::new(bytes.as_mut_slice());
    struct Checked<'a, 'b, C> {
        out: &'a mut std::io::Cursor<&'b mut [u8]>,
        stop: &'a mut C,
        seen: &'a mut bool,
    }
    impl<C: FnMut() -> bool> Write for Checked<'_, '_, C> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            for chunk in bytes.chunks(4096) {
                if (self.stop)() {
                    *self.seen = true;
                    return Err(std::io::Error::other("fixed path cancelled"));
                }
                self.out.write_all(chunk)?;
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut seen = false;
    let result = write_semantic_key_v1(
        key,
        &mut Checked {
            out: &mut cursor,
            stop: &mut write.cancelled,
            seen: &mut seen,
        },
    );
    if seen || (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    result.map_err(MemberRecordError::Io)?;
    if cursor.position() != size {
        return Err(ValueTreeError::Invalid("fixed path length").into());
    }
    Ok(charge.retain(bytes))
}
