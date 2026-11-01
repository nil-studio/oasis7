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
    occurrences: ReservedValue<Vec<CheckedFactOccurrence>>,
}
#[derive(Debug)]
pub(super) struct CheckedFactOccurrence {
    pub(super) kind: ExecutionFactKindV1,
    pub(super) id: [u8; 32],
    pub(super) bytes: u64,
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
    pub(super) fn checked_occurrence(
        &self,
        ordinal: usize,
        memory: &MemoryBudget,
    ) -> Result<Option<&CheckedFactOccurrence>, &'static str> {
        let required = self
            .occurrences
            .capacity()
            .checked_mul(std::mem::size_of::<CheckedFactOccurrence>())
            .ok_or("fact observation capacity overflow")?;
        if !self.occurrences.uses_budget(memory) || self.occurrences.reserved_bytes() < required {
            return Err("fact observation memory pool/charge");
        }
        Ok(self.occurrences.get(ordinal))
    }
}
#[derive(Clone, Copy)]
enum StreamStop {
    Cancelled,
    Limited,
    Interrupted,
}
struct HashSink<'a, C, H> {
    observe: H,
    hash: blake3::Hasher,
    bytes: u64,
    limit: u64,
    cancelled: &'a mut C,
    stop: Option<StreamStop>,
}
impl<C: FnMut() -> bool, H: FnMut(&[u8])> Write for HashSink<'_, C, H> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.stop.is_some() {
            return Err(std::io::Error::other("original stream stopped"));
        }
        // A source can catch an unwind from either callback and retry output.
        // Keep this stream poisoned until all callback/hash work completes.
        self.stop = Some(StreamStop::Interrupted);
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
        (self.observe)(&bytes[..count]);
        self.hash.update(&bytes[..count]);
        self.bytes += count as u64;
        self.stop = None;
        Ok(count)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        if self.stop.is_some() {
            Err(std::io::Error::other("original stream stopped"))
        } else {
            Ok(())
        }
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
    if objects.allocation_class() != MemoryClass::Ordinary
        || !objects.allocation_budget().same_pool(memory)
        || !core.facts.uses_budget(memory)
        || !core.catalog.uses_budget(memory)
    {
        return Err(CoreError::Invalid("fact source memory pools"));
    }
    let capacity = core.facts.facts.len();
    let observation_bytes = capacity
        .checked_mul(std::mem::size_of::<CheckedFactOccurrence>())
        .ok_or(CoreError::ResourceLimited("fact observation capacity"))?;
    let observations_charge = memory
        .reserve(observation_bytes, MemoryClass::Ordinary)
        .map_err(ExecutionError::Memory)?;
    let mut occurrences = Vec::new();
    occurrences
        .try_reserve_exact(capacity)
        .map_err(|_| CoreError::ResourceLimited("fact observation allocation"))?;
    if occurrences.capacity() > capacity {
        return Err(CoreError::ResourceLimited("fact observation capacity"));
    }
    let mut counts = [0u64; 3];
    let mut bytes = 0u64;
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
        bytes = bytes
            .checked_add(length)
            .ok_or(CoreError::ResourceLimited("fact byte total"))?;
        let index = match fact.kind {
            ExecutionFactKindV1::Input => 0,
            ExecutionFactKindV1::Result => 1,
            ExecutionFactKindV1::Event => 2,
        };
        counts[index] += 1;
        occurrences.push(CheckedFactOccurrence {
            kind: fact.kind,
            id: fact.id,
            bytes: length,
        });
    }
    if cancelled() {
        return Err(CoreError::Cancelled);
    }
    Ok(charge.retain(CheckedFactOriginals {
        body: core.body.digest()?,
        facts: core.body.facts,
        counts,
        bytes,
        occurrences: observations_charge.retain(occurrences),
    }))
}

/// Caller admits fixed hash metadata and IO allowance before invoking this.
pub(super) fn verify_original_stream<O: OriginalObjectStreamReader>(
    objects: &mut O,
    id: &[u8; 32],
    admitted: u64,
    memory: &MemoryBudget,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<u64, CoreError<O::Error>> {
    verify_original_stream_observed(
        objects,
        id,
        admitted,
        memory,
        MemoryClass::Ordinary,
        cancelled,
        |_| {},
    )
}

pub(super) fn verify_original_stream_observed<O: OriginalObjectStreamReader>(
    objects: &mut O,
    id: &[u8; 32],
    admitted: u64,
    memory: &MemoryBudget,
    class: MemoryClass,
    cancelled: &mut impl FnMut() -> bool,
    observe: impl FnMut(&[u8]),
) -> Result<u64, CoreError<O::Error>> {
    if !objects.allocation_budget().same_pool(memory) || objects.allocation_class() != class {
        return Err(CoreError::Invalid("original stream source pool/class"));
    }
    let mut sink = HashSink {
        observe,
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
        Some(StreamStop::Interrupted) => {
            return Err(CoreError::Invalid("original stream interrupted"));
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
