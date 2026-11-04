//! Canonical equality after an owner has admitted and decoded its closed DTO.
//! This helper neither decodes untrusted values nor establishes fact provenance.
use oasis7_formal_protocol::incremental_value::ValueTreeError;
use oasis7_proto::memory_budget::{
    MemoryBudget, MemoryClass, MemoryLimitExceeded, MemoryReservation,
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
    fn reserve(&self, n: usize) -> Result<Self::Charge, CanonicalCborStreamError> {
        self.memory.reserve(n, self.class).map_err(|e| {
            *self.error.borrow_mut() = Some(e);
            CanonicalCborStreamError::ResourceLimited("original canonical memory")
        })
    }
}
struct Compare<'a, 'b, C> {
    expected: &'a [u8],
    at: usize,
    cancelled: &'b RefCell<&'b mut C>,
    failed: bool,
}
impl<C: FnMut() -> bool> Write for Compare<'_, '_, C> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.failed {
            return Err(io::Error::other("original comparison stopped"));
        }
        self.failed = true;
        if (self.cancelled.borrow_mut())() {
            return Err(io::Error::other("original cancelled"));
        }
        let n = b.len().min(16384);
        let end = self
            .at
            .checked_add(n)
            .filter(|n| *n <= self.expected.len())
            .ok_or_else(|| io::Error::other("original length"))?;
        if self.expected[self.at..end] != b[..n] {
            return Err(io::Error::other("original canonical mismatch"));
        }
        self.at = end;
        self.failed = false;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.failed {
            Err(io::Error::other("original comparison stopped"))
        } else {
            Ok(())
        }
    }
}
pub fn check_decoded_canonical_original_v1<T: serde::Serialize, C: FnMut() -> bool>(
    value: &T,
    raw: &[u8],
    memory: &MemoryBudget,
    class: MemoryClass,
    work: &mut CanonicalCborWorkV1,
    cancelled: &mut C,
) -> Result<(), ValueTreeError> {
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    let _phase = memory.reserve(8192, class)?;
    let allocator = Allocator {
        memory,
        class,
        error: RefCell::new(None),
    };
    let empty = CanonicalCborWorkV1 {
        visits_remaining: 0,
        bytes_remaining: 0,
        max_depth: work.max_depth,
        max_map_keys: work.max_map_keys,
        max_key_bytes: work.max_key_bytes,
    };
    let cell = RefCell::new(cancelled);
    let mut context =
        CanonicalCborStreamContextV1::new(&allocator, std::mem::replace(work, empty), || {
            (cell.borrow_mut())()
        });
    let mut compare = Compare {
        expected: raw,
        at: 0,
        cancelled: &cell,
        failed: false,
    };
    let result = stream_canonical_cbor_v1(value, &mut context, &mut compare);
    *work = context.work;
    if (cell.borrow_mut())() {
        return Err(ValueTreeError::Cancelled);
    }
    if let Some(error) = allocator.error.borrow_mut().take() {
        return Err(ValueTreeError::Memory(error));
    }
    match result {
        Err(CanonicalCborStreamError::ResourceLimited(s)) => {
            return Err(ValueTreeError::ResourceLimited(s));
        }
        Err(CanonicalCborStreamError::Cancelled) => return Err(ValueTreeError::Cancelled),
        Err(_) => return Err(ValueTreeError::Invalid("original canonical mismatch")),
        Ok(summary)
            if compare.failed
                || compare.at != raw.len()
                || summary.encoded_bytes != raw.len() as u64 =>
        {
            return Err(ValueTreeError::Invalid("original canonical length"));
        }
        Ok(_) => {}
    }
    Ok(())
}
