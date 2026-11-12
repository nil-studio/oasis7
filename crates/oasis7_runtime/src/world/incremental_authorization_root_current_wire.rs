//! The maintained original ABI canonical serializer, under one Common child.
//! Both passes pay actual attempts; no ciborium Value/whole graph is cloned.
use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryReservation};
use oasis7_wasm_abi::{
    CanonicalCborAllocator, CanonicalCborStreamContextV1, CanonicalCborStreamError,
    CanonicalCborWorkV1, stream_canonical_cbor_v1,
};
use std::cell::RefCell;
struct Allocator {
    memory: MemoryBudget,
    class: MemoryClass,
    error: RefCell<Option<MemoryLimitExceeded>>,
}
impl CanonicalCborAllocator for Allocator {
    type Charge = MemoryReservation;
    fn reserve(&self, bytes: usize) -> Result<MemoryReservation, CanonicalCborStreamError> {
        self.memory.reserve(bytes, self.class).map_err(|e| {
            *self.error.borrow_mut() = Some(e);
            CanonicalCborStreamError::ResourceLimited("authorization canonical allocation")
        })
    }
}
pub(super) struct Measurement {
    pub bytes: u64,
    pub digest: [u8; 32],
}
struct Count(Sha256);
impl Write for Count {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.0.update(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct Attempts<'a, 's, W: ?Sized> {
    out: &'a mut W,
    stop: &'a RefCell<&'s mut dyn FnMut() -> bool>,
    attempts: u64,
    remaining: u64,
    bytes: u64,
    max: u64,
    cancelled: bool,
    limited: bool,
}
impl<W: Write + ?Sized> Write for Attempts<'_, '_, W> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            self.limited = true;
            return Err(io::Error::other("authorization serializer attempts"));
        }
        self.attempts = self.attempts.checked_add(1).ok_or(io::ErrorKind::Other)?;
        self.remaining -= 1;
        if (self.stop.borrow_mut())() {
            self.cancelled = true;
            return Err(io::Error::other("authorization serializer cancelled"));
        }
        let n = b.len().min(16 * 1024);
        if self
            .bytes
            .checked_add(n as u64)
            .filter(|n| *n <= self.max)
            .is_none()
        {
            self.limited = true;
            return Err(io::Error::other("authorization serializer output limit"));
        }
        let n = self.out.write(&b[..n])?;
        if n > b.len().min(16 * 1024) {
            return Err(io::Error::other("authorization serializer output count"));
        }
        self.bytes = self
            .bytes
            .checked_add(n as u64)
            .ok_or(io::ErrorKind::Other)?;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}
fn pass<T: serde::Serialize + ?Sized>(
    value: &T,
    out: &mut dyn Write,
    limits: AuthorizationRootCacheLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
    class: MemoryClass,
) -> Result<u64, AuthorizationRootCacheErrorV1> {
    common.consume_steps(1)?;
    let (memory, meter, stop) = common.parts();
    let allocator = Allocator {
        memory: memory.clone(),
        class,
        error: RefCell::new(None),
    };
    let lease = meter.begin_child()?;
    let before = (lease.remaining_steps(), lease.remaining_read_bytes());
    let visits = before.0 / 2;
    let stop = RefCell::new(stop);
    let mut writer = Attempts {
        out,
        stop: &stop,
        attempts: 0,
        remaining: visits,
        bytes: 0,
        max: limits.max_wire_bytes as u64,
        cancelled: false,
        limited: false,
    };
    let mut context = CanonicalCborStreamContextV1::new(
        &allocator,
        CanonicalCborWorkV1 {
            visits_remaining: visits,
            bytes_remaining: before.1,
            max_depth: limits.max_depth,
            max_map_keys: limits.max_map_keys,
            max_key_bytes: limits.max_key_bytes,
        },
        || (stop.borrow_mut())(),
    );
    let result = stream_canonical_cbor_v1(value, &mut context, &mut writer);
    let steps = visits
        .checked_sub(context.work.visits_remaining)
        .and_then(|n| n.checked_add(writer.attempts));
    let bytes = before.1.checked_sub(context.work.bytes_remaining);
    let debit = match steps.zip(bytes) {
        Some((s, b)) => lease.consume(s, b),
        None => {
            drop(lease);
            return Err(invalid("authorization serializer counters"));
        }
    };
    lease.finish();
    if let Some(error) = allocator.error.borrow_mut().take() {
        return Err(error.into());
    }
    if writer.cancelled || matches!(result, Err(CanonicalCborStreamError::Cancelled)) {
        return Err(ValueTreeError::Cancelled.into());
    }
    if writer.limited {
        return Err(limited());
    }
    result.map_err(AuthorizationRootCacheErrorV1::Canonical)?;
    debit?;
    let written = writer.bytes;
    drop(context);
    drop(writer);
    drop(stop);
    common.consume_steps(0)?;
    Ok(written)
}
pub(super) fn measure<T: serde::Serialize + ?Sized>(
    value: &T,
    limits: AuthorizationRootCacheLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
    class: MemoryClass,
) -> Result<Measurement, AuthorizationRootCacheErrorV1> {
    let mut counter = Count(Sha256::new());
    let bytes = pass(value, &mut counter, limits, common, class)?;
    Ok(Measurement {
        bytes,
        digest: counter.0.finalize().into(),
    })
}
pub(super) fn encode<T: serde::Serialize + ?Sized>(
    value: &T,
    limits: AuthorizationRootCacheLimitsV1,
    common: &mut FormalBoundedContextV1<'_>,
    class: MemoryClass,
) -> Result<ReservedValue<Vec<u8>>, AuthorizationRootCacheErrorV1> {
    let measured = measure(value, limits, common, class)?;
    let len = usize::try_from(measured.bytes).map_err(|_| limited())?;
    let fee = common.memory().reserve(
        len.checked_add(std::mem::size_of::<Vec<u8>>())
            .ok_or_else(limited)?,
        class,
    )?;
    let mut raw = Vec::new();
    raw.try_reserve_exact(len).map_err(|_| limited())?;
    if raw.capacity() > len {
        return Err(limited());
    }
    struct Exact<'a> {
        raw: &'a mut Vec<u8>,
        max: usize,
        hash: Sha256,
    }
    impl Write for Exact<'_> {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            if b.len() > self.max - self.raw.len() {
                return Err(io::Error::other(
                    "authorization changed serialization length",
                ));
            }
            self.hash.update(b);
            self.raw.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut exact = Exact {
        raw: &mut raw,
        max: len,
        hash: Sha256::new(),
    };
    let written = pass(value, &mut exact, limits, common, class)?;
    if written != measured.bytes || <[u8; 32]>::from(exact.hash.finalize()) != measured.digest {
        return Err(invalid("authorization deterministic serialization"));
    }
    Ok(fee.retain(raw))
}
