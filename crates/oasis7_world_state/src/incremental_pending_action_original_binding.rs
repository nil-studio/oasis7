//! Bind a charged typed Action projection to actual canonical original bytes.
//! A byte/type proof is separate from authority, retention and durable replicas.
use crate::incremental_runtime_pending_action_record::RecoveredPendingActionRecordV1;
use oasis7_formal_protocol::{
    incremental_artifacts::OriginalArtifactRefV1, incremental_material::OriginalObjectStreamReader,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{
    MemoryBudget, MemoryClass, MemoryLimitExceeded, MemoryReservation, ReservedValue,
};
use oasis7_wasm_abi::{
    CanonicalCborAllocator, CanonicalCborStreamContextV1, CanonicalCborStreamError,
    CanonicalCborWorkV1, stream_canonical_cbor_v1,
};
use std::{
    cell::RefCell,
    io::{self, Write},
};

pub struct PendingActionOriginalWorkV1 {
    pub objects_remaining: u64,
    pub original_bytes_remaining: u64,
    pub max_original_bytes: u64,
    pub canonical: CanonicalCborWorkV1,
}
#[derive(Debug)]
pub enum PendingActionOriginalErrorV1<E> {
    Value(ValueTreeError),
    Encoding(CanonicalCborStreamError),
    Source(E),
    Invalid(&'static str),
}
impl<E> From<ValueTreeError> for PendingActionOriginalErrorV1<E> {
    fn from(error: ValueTreeError) -> Self {
        Self::Value(error)
    }
}
impl<E: std::fmt::Display> std::fmt::Display for PendingActionOriginalErrorV1<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Value(e) => e.fmt(f),
            Self::Encoding(e) => e.fmt(f),
            Self::Source(e) => e.fmt(f),
            Self::Invalid(e) => f.write_str(e),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for PendingActionOriginalErrorV1<E> {}
pub struct CheckedPendingActionOriginalV1 {
    reference: OriginalArtifactRefV1,
    class: MemoryClass,
}
impl CheckedPendingActionOriginalV1 {
    pub fn original(&self) -> OriginalArtifactRefV1 {
        self.reference
    }
    pub fn class(&self) -> MemoryClass {
        self.class
    }
}
struct Allocator<'a> {
    memory: &'a MemoryBudget,
    class: MemoryClass,
    error: RefCell<Option<MemoryLimitExceeded>>,
}
impl CanonicalCborAllocator for Allocator<'_> {
    type Charge = MemoryReservation;
    fn reserve(&self, bytes: usize) -> Result<Self::Charge, CanonicalCborStreamError> {
        self.memory.reserve(bytes, self.class).map_err(|error| {
            *self.error.borrow_mut() = Some(error);
            CanonicalCborStreamError::ResourceLimited("pending original canonical heap")
        })
    }
}
#[derive(Clone, Copy)]
enum Stop {
    Cancelled,
    Limited,
    Interrupted,
}
struct HashSink<'a> {
    hash: blake3::Hasher,
    bytes: u64,
    max: u64,
    stop: Option<Stop>,
    cancelled: &'a mut dyn FnMut() -> bool,
}
impl HashSink<'_> {
    fn failure(&self) -> Option<ValueTreeError> {
        self.stop.map(|stop| match stop {
            Stop::Cancelled => ValueTreeError::Cancelled,
            Stop::Limited => ValueTreeError::ResourceLimited("pending original bytes"),
            Stop::Interrupted => ValueTreeError::Invalid("pending original interrupted"),
        })
    }
}
impl Write for HashSink<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.stop.is_some() {
            return Err(io::Error::other("pending original stream stopped"));
        }
        self.stop = Some(Stop::Interrupted);
        if (self.cancelled)() {
            self.stop = Some(Stop::Cancelled);
            return Err(io::Error::other("pending original cancelled"));
        }
        let count = bytes.len().min(16 << 10);
        if self
            .bytes
            .checked_add(count as u64)
            .is_none_or(|n| n > self.max)
        {
            self.stop = Some(Stop::Limited);
            return Err(io::Error::other("pending original byte bound"));
        }
        self.hash.update(&bytes[..count]);
        self.bytes += count as u64;
        self.stop = None;
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.stop.is_some() {
            Err(io::Error::other("pending original stream stopped"))
        } else {
            Ok(())
        }
    }
}
/// No untrusted original DTO is deserialized. The complete typed projection is
/// canonically encoded to a bounded hash sink, then the actual original source
/// is independently streamed and compared by exact length and content ID.
pub fn verify_pending_action_original_v1<O: OriginalObjectStreamReader>(
    record: &RecoveredPendingActionRecordV1,
    objects: &mut O,
    memory: &MemoryBudget,
    work: &mut PendingActionOriginalWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<CheckedPendingActionOriginalV1>, PendingActionOriginalErrorV1<O::Error>> {
    use PendingActionOriginalErrorV1 as Error;
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let class = record.class();
    if !record.uses_budget(memory)
        || !objects.allocation_budget().same_pool(memory)
        || objects.allocation_class() != class
    {
        return Err(Error::Invalid("pending original pool/class"));
    }
    let reference = record.value().original()?;
    if reference.byte_len > work.max_original_bytes
        || reference.byte_len > work.original_bytes_remaining
    {
        return Err(ValueTreeError::ResourceLimited("pending original allowance").into());
    }
    work.objects_remaining =
        work.objects_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited(
                "pending original occurrences",
            ))?;
    work.original_bytes_remaining -= reference.byte_len;
    let _phase = memory
        .reserve(16 << 10, class)
        .map_err(ValueTreeError::Memory)?;
    let retained = memory.reserve(512, class).map_err(ValueTreeError::Memory)?;
    let allocator = Allocator {
        memory,
        class,
        error: RefCell::new(None),
    };
    let placeholder = CanonicalCborWorkV1 {
        visits_remaining: 0,
        bytes_remaining: 0,
        max_depth: work.canonical.max_depth,
        max_map_keys: work.canonical.max_map_keys,
        max_key_bytes: work.canonical.max_key_bytes,
    };
    let stop = RefCell::new(&mut cancelled);
    let mut encoding = CanonicalCborStreamContextV1::new(
        &allocator,
        std::mem::replace(&mut work.canonical, placeholder),
        || (stop.borrow_mut())(),
    );
    let mut sink = HashSink {
        hash: blake3::Hasher::new(),
        bytes: 0,
        max: reference.byte_len,
        stop: None,
        cancelled: &mut || (stop.borrow_mut())(),
    };
    let result = stream_canonical_cbor_v1(record.value().envelope(), &mut encoding, &mut sink);
    work.canonical = encoding.work;
    if let Some(error) = allocator.error.borrow_mut().take() {
        return Err(ValueTreeError::Memory(error).into());
    }
    if let Some(error) = sink.failure() {
        return Err(error.into());
    }
    let result = result.map_err(Error::Encoding)?;
    if result.encoded_bytes != reference.byte_len
        || sink.bytes != reference.byte_len
        || sink.hash.finalize().as_bytes() != &reference.id
    {
        return Err(Error::Invalid(
            "pending projection canonical original binding",
        ));
    }
    drop(sink);
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let mut sink = HashSink {
        hash: blake3::Hasher::new(),
        bytes: 0,
        max: reference.byte_len,
        stop: None,
        cancelled: &mut cancelled,
    };
    let streamed = objects.stream_original(&reference.id, reference.byte_len, memory, &mut sink);
    if let Some(error) = sink.failure() {
        return Err(error.into());
    }
    let length = streamed.map_err(Error::Source)?;
    if length != reference.byte_len
        || sink.bytes != length
        || sink.hash.finalize().as_bytes() != &reference.id
    {
        return Err(Error::Invalid("pending actual original length/hash"));
    }
    drop(sink);
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(retained.retain(CheckedPendingActionOriginalV1 { reference, class }))
}

#[cfg(test)]
#[path = "incremental_pending_action_original_binding_tests.rs"]
pub(crate) mod tests;
