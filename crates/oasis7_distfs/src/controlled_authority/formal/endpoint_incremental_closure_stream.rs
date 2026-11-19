//! One dynamic continuation original, authenticated with bounded writes.
use super::*;
use oasis7_formal_protocol::incremental_material::OriginalArtifactBudget;
use std::io::{self, Write};
struct HashSink<C> {
    hash: blake3::Hasher,
    bytes: u64,
    expected: u64,
    cancelled: C,
    failed: bool,
}
impl<C: FnMut() -> bool> Write for HashSink<C> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.failed {
            return Err(io::Error::other("continuation sink poisoned"));
        }
        self.failed = true;
        let bytes = &bytes[..bytes.len().min(64 << 10)];
        if (self.cancelled)() {
            return Err(io::Error::other("continuation cancelled"));
        }
        let next = self
            .bytes
            .checked_add(bytes.len() as u64)
            .filter(|n| *n <= self.expected)
            .ok_or_else(|| io::Error::other("continuation output length"))?;
        self.hash.update(bytes);
        self.bytes = next;
        self.failed = false;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.failed {
            Err(io::Error::other("continuation sink poisoned"))
        } else {
            Ok(())
        }
    }
}
pub(super) fn verify_original<O: OriginalObjectStreamReader>(
    entry: OriginalArtifactRefV1,
    objects: &mut O,
    memory: &MemoryBudget,
    work: &mut OriginalArtifactBudget,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<(), FormalError> {
    if cancelled() {
        return Err(invalid("continuation cancelled"));
    }
    if entry.byte_len > work.max_object_bytes || entry.byte_len > work.bytes_remaining {
        return Err(invalid("continuation byte allowance"));
    }
    work.objects_remaining = work
        .objects_remaining
        .checked_sub(1)
        .ok_or_else(|| invalid("continuation object allowance"))?;
    work.bytes_remaining -= entry.byte_len;
    let _phase = memory
        .reserve(4096, MemoryClass::Ordinary)
        .map_err(|_| invalid("continuation hash admission"))?;
    let mut sink = HashSink {
        hash: blake3::Hasher::new(),
        bytes: 0,
        expected: entry.byte_len,
        cancelled: &mut *cancelled,
        failed: false,
    };
    let length = objects
        .stream_original(&entry.id, entry.byte_len, memory, &mut sink)
        .map_err(|_| invalid("continuation actual original stream"))?;
    if sink.failed
        || sink.bytes != entry.byte_len
        || length != entry.byte_len
        || sink.hash.finalize().as_bytes() != &entry.id
    {
        return Err(invalid("continuation original hash/length"));
    }
    if cancelled() {
        return Err(invalid("continuation final cancellation"));
    }
    Ok(())
}
