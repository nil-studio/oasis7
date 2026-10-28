//! Exact original bytes for the current ordered facts, without materializing
//! entire inputs/responses/events. Byte identity is not semantic validation.
use super::*;
use std::io::Write;

/// Output is provisional until the source returns successfully. Implementors
/// must honor Write's partial-write/error contract and their own IO allowances.
pub trait OriginalObjectStreamReader: OriginalObjectReader {
    fn stream_original(
        &mut self,
        id: &[u8; 32],
        max_bytes: u64,
        memory: &MemoryBudget,
        output: &mut dyn Write,
    ) -> Result<u64, Self::Error>;
}
pub struct FactBudget {
    pub facts_remaining: u64,
    pub original_bytes_remaining: u64,
    pub max_fact_bytes: u64,
}
/// Attests only raw byte identity of every occurrence in this exact FactSet.
/// No constructor/Clone/receipt/acceptance capability is exposed. Payload type,
/// business semantics and the other complete-material obligations remain.
#[derive(Debug)]
pub struct CheckedFactOriginals {
    body: [u8; 32],
    facts: [u8; 32],
    counts: [u64; 3],
    bytes: u64,
}
impl CheckedFactOriginals {
    pub fn body_id(&self) -> [u8; 32] {
        self.body
    }
    pub fn fact_set_id(&self) -> [u8; 32] {
        self.facts
    }
    /// Ordered-kind totals: Input, Result, Event. Duplicates count as occurrences.
    pub fn counts(&self) -> [u64; 3] {
        self.counts
    }
    pub fn original_bytes(&self) -> u64 {
        self.bytes
    }
}
#[derive(Clone, Copy)]
enum StreamStop {
    Cancelled,
    Limited,
}
struct HashSink<'a, C> {
    hash: blake3::Hasher,
    bytes: u64,
    limit: u64,
    cancelled: &'a mut C,
    stop: Option<StreamStop>,
}
impl<C: FnMut() -> bool> Write for HashSink<'_, C> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.stop.is_some() {
            return Err(std::io::Error::other("original stream stopped"));
        }
        if (self.cancelled)() {
            self.stop = Some(StreamStop::Cancelled);
            return Err(std::io::Error::other("original stream cancelled"));
        }
        // Bound hash/cancellation work even if an adapter passes a huge slice.
        let count = bytes.len().min(crate::incremental_value::VALUE_CHUNK_BYTES);
        if self
            .bytes
            .checked_add(count as u64)
            .is_none_or(|end| end > self.limit)
        {
            self.stop = Some(StreamStop::Limited);
            return Err(std::io::Error::other("original stream byte allowance"));
        }
        self.hash.update(&bytes[..count]);
        self.bytes += count as u64;
        Ok(count)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub fn verify_fact_originals<O: OriginalObjectStreamReader>(
    core: &CheckedCommitCore,
    objects: &mut O,
    memory: &MemoryBudget,
    budget: &mut FactBudget,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CheckedFactOriginals>, CoreError<O::Error>> {
    let charge = memory
        .reserve(16 * 1024, MemoryClass::Ordinary)
        .map_err(ExecutionError::Memory)?;
    if !objects.allocation_budget().same_pool(memory)
        || !core.facts.uses_budget(memory)
        || !core.catalog.uses_budget(memory)
    {
        return Err(CoreError::Invalid("fact source memory pools"));
    }
    let mut result = CheckedFactOriginals {
        body: core.body.digest()?,
        facts: core.body.facts,
        counts: [0; 3],
        bytes: 0,
    };
    for fact in &core.facts.facts {
        if cancelled() {
            return Err(CoreError::Cancelled);
        }
        if budget.facts_remaining == 0 {
            return Err(CoreError::ResourceLimited("fact occurrences"));
        }
        let admitted = budget.max_fact_bytes.min(budget.original_bytes_remaining);
        budget.facts_remaining -= 1;
        budget.original_bytes_remaining -= admitted;
        let length = verify_original_stream(objects, &fact.id, admitted, memory, &mut cancelled)?;
        budget.original_bytes_remaining += admitted - length;
        result.bytes = result
            .bytes
            .checked_add(length)
            .ok_or(CoreError::ResourceLimited("fact byte total"))?;
        let index = match fact.kind {
            ExecutionFactKindV1::Input => 0,
            ExecutionFactKindV1::Result => 1,
            ExecutionFactKindV1::Event => 2,
        };
        result.counts[index] += 1;
    }
    Ok(charge.retain(result))
}

/// Caller admits fixed hash metadata and IO allowance before invoking this.
pub(super) fn verify_original_stream<O: OriginalObjectStreamReader>(
    objects: &mut O,
    id: &[u8; 32],
    admitted: u64,
    memory: &MemoryBudget,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<u64, CoreError<O::Error>> {
    let mut sink = HashSink {
        hash: blake3::Hasher::new(),
        bytes: 0,
        limit: admitted,
        cancelled,
        stop: None,
    };
    let streamed = objects.stream_original(id, admitted, memory, &mut sink);
    match sink.stop {
        Some(StreamStop::Cancelled) => return Err(CoreError::Cancelled),
        Some(StreamStop::Limited) => {
            return Err(CoreError::ResourceLimited("original stream bytes"));
        }
        None => {}
    }
    let length = streamed.map_err(CoreError::Object)?;
    if length != sink.bytes || sink.hash.finalize().as_bytes() != id {
        return Err(CoreError::Invalid("original stream length/hash"));
    }
    if cancelled() {
        return Err(CoreError::Cancelled);
    }
    Ok(length)
}
