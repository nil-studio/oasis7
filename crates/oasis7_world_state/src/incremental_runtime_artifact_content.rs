//! Original Runtime artifact content. Raw identity is not execution authority.
use oasis7_formal_protocol::{
    incremental_artifacts::OriginalArtifactRefV1,
    incremental_material::OriginalObjectStreamReader,
    incremental_value::{VALUE_CHUNK_BYTES, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::io::{self, Write};
const ORIGINAL: &[u8] = b"oasis7.iws.module-artifact-original.v1\0";
pub const RUNTIME_ARTIFACT_ORIGINAL_BYTES: usize = ORIGINAL.len() + 40;

pub fn runtime_artifact_original_bytes_v1(
    original: OriginalArtifactRefV1,
) -> [u8; RUNTIME_ARTIFACT_ORIGINAL_BYTES] {
    let mut bytes = [0; RUNTIME_ARTIFACT_ORIGINAL_BYTES];
    bytes[..ORIGINAL.len()].copy_from_slice(ORIGINAL);
    bytes[ORIGINAL.len()..ORIGINAL.len() + 32].copy_from_slice(&original.id);
    bytes[ORIGINAL.len() + 32..].copy_from_slice(&original.byte_len.to_be_bytes());
    bytes
}
pub fn runtime_artifact_original_from_v1(
    bytes: &[u8],
) -> Result<OriginalArtifactRefV1, ValueTreeError> {
    if bytes.len() != RUNTIME_ARTIFACT_ORIGINAL_BYTES || !bytes.starts_with(ORIGINAL) {
        return Err(ValueTreeError::Invalid("Runtime artifact original framing"));
    }
    Ok(OriginalArtifactRefV1 {
        id: bytes[ORIGINAL.len()..ORIGINAL.len() + 32]
            .try_into()
            .unwrap(),
        byte_len: u64::from_be_bytes(bytes[ORIGINAL.len() + 32..].try_into().unwrap()),
    })
}

pub struct RuntimeArtifactOriginalWorkV1 {
    pub objects_remaining: u64,
    pub bytes_remaining: u64,
    pub max_object_bytes: u64,
}
#[derive(Debug)]
pub enum RuntimeArtifactOriginalErrorV1<E> {
    Source(E),
    Value(ValueTreeError),
}
impl<E> From<ValueTreeError> for RuntimeArtifactOriginalErrorV1<E> {
    fn from(error: ValueTreeError) -> Self {
        Self::Value(error)
    }
}
/// Private construction proves only the exact raw original ID and length.
/// Its containing state/catalog/accepted head and physical closure are separate.
pub struct CheckedRuntimeArtifactOriginalV1 {
    original: OriginalArtifactRefV1,
}
impl CheckedRuntimeArtifactOriginalV1 {
    pub fn original(&self) -> OriginalArtifactRefV1 {
        self.original
    }
}
struct HashOutput<'a, C> {
    hash: blake3::Hasher,
    bytes: u64,
    limit: u64,
    stopped: Option<ValueTreeError>,
    cancelled: &'a mut C,
}
impl<C: FnMut() -> bool> Write for HashOutput<'_, C> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.stopped.is_some() {
            return Err(io::Error::other("Runtime artifact stream stopped"));
        }
        // Sticky before calling external code, including a caught unwind.
        self.stopped = Some(ValueTreeError::Invalid("Runtime artifact interrupted"));
        if (self.cancelled)() {
            self.stopped = Some(ValueTreeError::Cancelled);
            return Err(io::Error::other("Runtime artifact cancelled"));
        }
        let n = bytes.len().min(VALUE_CHUNK_BYTES);
        if self
            .bytes
            .checked_add(n as u64)
            .is_none_or(|n| n > self.limit)
        {
            self.stopped = Some(ValueTreeError::ResourceLimited(
                "Runtime artifact stream bytes",
            ));
            return Err(io::Error::other("Runtime artifact bytes"));
        }
        self.hash.update(&bytes[..n]);
        self.bytes += n as u64;
        self.stopped = None;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.stopped.is_some() {
            return Err(io::Error::other("Runtime artifact stopped"));
        }
        self.stopped = Some(ValueTreeError::Invalid("Runtime artifact interrupted"));
        if (self.cancelled)() {
            self.stopped = Some(ValueTreeError::Cancelled);
            return Err(io::Error::other("Runtime artifact cancelled"));
        }
        self.stopped = None;
        Ok(())
    }
}
pub fn verify_runtime_artifact_original_v1<O: OriginalObjectStreamReader, C: FnMut() -> bool>(
    original: OriginalArtifactRefV1,
    source: &mut O,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut RuntimeArtifactOriginalWorkV1,
    cancelled: &mut C,
) -> Result<ReservedValue<CheckedRuntimeArtifactOriginalV1>, RuntimeArtifactOriginalErrorV1<O::Error>>
{
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if !source.allocation_budget().same_pool(memory) || source.allocation_class() != class {
        return Err(ValueTreeError::Invalid("Runtime artifact source pool/class").into());
    }
    if work.objects_remaining == 0
        || original.byte_len > work.max_object_bytes
        || original.byte_len > work.bytes_remaining
    {
        return Err(ValueTreeError::ResourceLimited("Runtime artifact original allowance").into());
    }
    let retained = memory.reserve(512, class).map_err(ValueTreeError::Memory)?;
    work.objects_remaining -= 1;
    work.bytes_remaining -= original.byte_len;
    let mut output = HashOutput {
        hash: blake3::Hasher::new(),
        bytes: 0,
        limit: original.byte_len,
        stopped: None,
        cancelled,
    };
    let result = source.stream_original(&original.id, original.byte_len, memory, &mut output);
    if let Some(error) = output.stopped {
        return Err(error.into());
    }
    if (output.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let reported = result.map_err(RuntimeArtifactOriginalErrorV1::Source)?;
    if reported != original.byte_len
        || output.bytes != original.byte_len
        || output.hash.finalize().as_bytes() != &original.id
    {
        return Err(ValueTreeError::Invalid("Runtime artifact original identity/length").into());
    }
    Ok(retained.retain(CheckedRuntimeArtifactOriginalV1 { original }))
}

#[cfg(test)]
#[path = "incremental_runtime_artifact_content_tests.rs"]
mod tests;

#[path = "incremental_runtime_artifact_content_genesis.rs"]
mod genesis;
pub use genesis::*;
