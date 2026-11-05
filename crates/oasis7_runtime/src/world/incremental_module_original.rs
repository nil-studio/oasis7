//! Materialize only the selected verified original for the existing Arc WASM ABI.
use oasis7_formal_protocol::{
    incremental_artifacts::OriginalArtifactRefV1,
    incremental_material::{OriginalObjectReader, OriginalObjectStreamReader},
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::incremental_runtime_artifact_content::{
    RuntimeArtifactOriginalErrorV1, RuntimeArtifactOriginalWorkV1,
    verify_runtime_artifact_original_v1,
};
use std::{
    io::{self, Write},
    sync::Arc,
};

struct Capture<'a, O> {
    source: &'a mut O,
    bytes: Vec<u8>,
    limit: usize,
}
struct Tee<'a> {
    hash: &'a mut dyn Write,
    bytes: &'a mut Vec<u8>,
    limit: usize,
}
impl Write for Tee<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let n = self.hash.write(bytes)?;
        if n > bytes.len()
            || self
                .bytes
                .len()
                .checked_add(n)
                .is_none_or(|n| n > self.limit)
        {
            return Err(io::Error::other("selected WASM capture bound"));
        }
        self.bytes.extend_from_slice(&bytes[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.hash.flush()
    }
}
impl<O: OriginalObjectReader> OriginalObjectReader for Capture<'_, O> {
    type Error = O::Error;
    fn allocation_budget(&self) -> &MemoryBudget {
        self.source.allocation_budget()
    }
    fn allocation_class(&self) -> MemoryClass {
        self.source.allocation_class()
    }
    fn read_original(
        &mut self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, Self::Error> {
        self.source.read_original(id, max, memory)
    }
}
impl<O: OriginalObjectStreamReader> OriginalObjectStreamReader for Capture<'_, O> {
    fn stream_original(
        &mut self,
        id: &[u8; 32],
        max: u64,
        memory: &MemoryBudget,
        output: &mut dyn Write,
    ) -> Result<u64, Self::Error> {
        self.source.stream_original(
            id,
            max,
            memory,
            &mut Tee {
                hash: output,
                bytes: &mut self.bytes,
                limit: self.limit,
            },
        )
    }
}
type SelectedOriginalResult<E> =
    Result<(Arc<[u8]>, MemoryReservation), RuntimeArtifactOriginalErrorV1<E>>;
pub(super) fn selected_original<O: OriginalObjectStreamReader, C: FnMut() -> bool>(
    original: OriginalArtifactRefV1,
    source: &mut O,
    memory: &MemoryBudget,
    work: &mut RuntimeArtifactOriginalWorkV1,
    cancelled: &mut C,
) -> SelectedOriginalResult<O::Error> {
    let n = usize::try_from(original.byte_len)
        .map_err(|_| ValueTreeError::ResourceLimited("selected WASM address space"))?;
    // Vec and Arc may coexist during conversion. Keep this full admission for
    // the returned Arc lifetime; the existing sandbox ABI requires Arc<[u8]>.
    let admission = n
        .checked_mul(2)
        .and_then(|n| n.checked_add(1024))
        .ok_or(ValueTreeError::ResourceLimited("selected WASM allocation"))?;
    let charge = memory
        .reserve(admission, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut capture = Capture {
        source,
        bytes: Vec::with_capacity(n),
        limit: n,
    };
    let checked = verify_runtime_artifact_original_v1(
        original,
        &mut capture,
        memory,
        MemoryClass::Ordinary,
        work,
        cancelled,
    )?;
    if checked.original() != original || capture.bytes.len() != n {
        return Err(ValueTreeError::Invalid("selected WASM original capture").into());
    }
    let bytes = Arc::from(capture.bytes);
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok((bytes, charge))
}
