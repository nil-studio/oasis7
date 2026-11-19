//! Actual provisional artifact persistence and exact readback. No admission cap.
use super::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_artifacts::OriginalArtifactRefV1, incremental_material::OriginalObjectStreamReader,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::MemoryClass;
use std::{
    cell::RefCell,
    io::{self, Read, Write},
};
#[derive(Debug)]
pub(in crate::world) enum ArtifactOriginalWriteErrorV1<E> {
    Resource(BodyMutationError),
    Storage(E),
    Unsupported,
}
struct MeteredInput<'a, 'b, 'common, 'ctx> {
    bytes: &'a [u8],
    position: usize,
    common: &'b RefCell<&'common mut FormalBoundedContextV1<'ctx>>,
    first: &'b RefCell<Option<BodyMutationError>>,
}
impl Read for MeteredInput<'_, '_, '_, '_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let count = output.len().min(8192).min(self.bytes.len() - self.position);
        if self.first.borrow().is_some() {
            return Err(io::Error::other("artifact work stopped"));
        }
        if let Err(error) = self
            .common
            .borrow_mut()
            .charge(1 + count as u64, count as u64)
        {
            *self.first.borrow_mut() = Some(BodyMutationError::Original(Box::new(error)));
            return Err(io::Error::other("artifact input work"));
        }
        output[..count].copy_from_slice(&self.bytes[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}
struct ExactOutput<'a, 'b, 'common, 'ctx> {
    bytes: &'a [u8],
    position: usize,
    common: &'b RefCell<&'common mut FormalBoundedContextV1<'ctx>>,
    first: &'b RefCell<Option<BodyMutationError>>,
}
impl Write for ExactOutput<'_, '_, '_, '_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let result = (|| {
            if self.first.borrow().is_some() {
                return Err(BodyMutationError::Invalid("artifact work stopped"));
            }
            self.common
                .borrow_mut()
                .charge(1 + bytes.len() as u64, bytes.len() as u64)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            let end = self
                .position
                .checked_add(bytes.len())
                .ok_or(BodyMutationError::Invalid("artifact readback length"))?;
            if self.bytes.get(self.position..end) != Some(bytes) {
                return Err(BodyMutationError::Invalid("artifact readback bytes"));
            }
            self.position = end;
            Ok(())
        })();
        if let Err(error) = result {
            if self.first.borrow().is_none() {
                *self.first.borrow_mut() = Some(error)
            }
            return Err(io::Error::other("artifact readback stopped"));
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
/// The supplied source must be the actual Root context's original IO session.
/// Raw bytes are already selected/caller-admitted; this returns only byte data.
pub(in crate::world) fn persist_artifact_original_data_v1<O: OriginalObjectStreamReader>(
    originals: &mut O,
    bytes: &[u8],
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<OriginalArtifactRefV1, ArtifactOriginalWriteErrorV1<O::Error>> {
    use ArtifactOriginalWriteErrorV1 as Error;
    if originals.allocation_class() != MemoryClass::Ordinary
        || !originals.allocation_budget().same_pool(common.memory())
    {
        return Err(Error::Resource(BodyMutationError::Invalid(
            "artifact original source pool/class",
        )));
    }
    let memory = common.memory().clone();
    let _scope = memory
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)
        .map_err(BodyMutationError::from)
        .map_err(Error::Resource)?;
    let mut hash = blake3::Hasher::new();
    for chunk in bytes.chunks(8192) {
        common
            .charge(1 + chunk.len() as u64, chunk.len() as u64)
            .map_err(|e| Error::Resource(BodyMutationError::Original(Box::new(e))))?;
        hash.update(chunk);
    }
    let id = *hash.finalize().as_bytes();
    let length = u64::try_from(bytes.len())
        .map_err(|_| Error::Resource(BodyMutationError::Invalid("artifact original width")))?;
    let first = RefCell::new(None);
    let common = RefCell::new(common);
    let write_result = {
        let writer = originals.original_writer().ok_or(Error::Unsupported)?;
        let mut input = MeteredInput {
            bytes,
            position: 0,
            common: &common,
            first: &first,
        };
        let result = writer.write_original(&mut input, length, id, &memory);
        (result, input.position)
    };
    if let Some(error) = first.take() {
        return Err(Error::Resource(error));
    }
    let written = write_result.0.map_err(Error::Storage)?;
    if write_result.1 != bytes.len() {
        return Err(Error::Resource(BodyMutationError::Invalid(
            "artifact writer did not consume original",
        )));
    }
    if written != id {
        return Err(Error::Resource(BodyMutationError::Invalid(
            "artifact persisted id",
        )));
    }
    let mut output = ExactOutput {
        bytes,
        position: 0,
        common: &common,
        first: &first,
    };
    let result = originals.stream_original(&id, length, &memory, &mut output);
    if let Some(error) = first.take() {
        return Err(Error::Resource(error));
    }
    let read = result.map_err(Error::Storage)?;
    if read != length || output.position != bytes.len() {
        return Err(Error::Resource(BodyMutationError::Invalid(
            "artifact readback length",
        )));
    }
    Ok(OriginalArtifactRefV1 {
        id,
        byte_len: length,
    })
}
