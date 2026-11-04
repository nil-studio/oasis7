//! Private shared streaming/hash engine; only concrete owner wrappers mint proof types.
use super::{PendingActionOriginalErrorV1, PendingActionOriginalWorkV1};
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
pub(super) enum Stop {
    Cancelled,
    Limited,
    Interrupted,
}
pub(super) struct HashSink<'a> {
    pub(super) hash: blake3::Hasher,
    pub(super) bytes: u64,
    pub(super) max: u64,
    pub(super) stop: Option<Stop>,
    pub(super) cancelled: &'a mut dyn FnMut() -> bool,
}
impl HashSink<'_> {
    pub(super) fn failure(&self) -> Option<ValueTreeError> {
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn verify_typed_original_v1<T: serde::Serialize, V, O: OriginalObjectStreamReader>(
    typed: &T,
    id: [u8; 32],
    expected_len: Option<u64>,
    objects: &mut O,
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut PendingActionOriginalWorkV1,
    mut cancelled: impl FnMut() -> bool,
    mint: impl FnOnce(OriginalArtifactRefV1, MemoryClass) -> V,
) -> Result<ReservedValue<V>, PendingActionOriginalErrorV1<O::Error>> {
    use PendingActionOriginalErrorV1 as Error;
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if !objects.allocation_budget().same_pool(memory) || objects.allocation_class() != class {
        return Err(Error::Invalid("pending original pool/class"));
    }
    let max = work.max_original_bytes.min(work.original_bytes_remaining);
    if expected_len.is_some_and(|n| n > max) {
        return Err(ValueTreeError::ResourceLimited("pending original allowance").into());
    }
    work.objects_remaining =
        work.objects_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited(
                "pending original occurrences",
            ))?;
    if let Some(len) = expected_len {
        work.original_bytes_remaining -= len;
    }
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
        || stop.borrow_mut()(),
    );
    let mut sink = HashSink {
        hash: blake3::Hasher::new(),
        bytes: 0,
        max: expected_len.unwrap_or(max),
        stop: None,
        cancelled: &mut || stop.borrow_mut()(),
    };
    let encoded = stream_canonical_cbor_v1(typed, &mut encoding, &mut sink);
    work.canonical = encoding.work;
    if let Some(error) = allocator.error.borrow_mut().take() {
        return Err(ValueTreeError::Memory(error).into());
    }
    if let Some(error) = sink.failure() {
        return Err(error.into());
    }
    let encoded = encoded.map_err(Error::Encoding)?;
    if encoded.encoded_bytes != sink.bytes
        || expected_len.is_some_and(|n| n != sink.bytes)
        || sink.hash.finalize().as_bytes() != &id
    {
        return Err(Error::Invalid(
            "pending projection canonical original binding",
        ));
    }
    let reference = OriginalArtifactRefV1 {
        id,
        byte_len: sink.bytes,
    };
    drop(sink);
    if expected_len.is_none() {
        work.original_bytes_remaining -= reference.byte_len;
    }
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
    Ok(retained.retain(mint(reference, class)))
}
