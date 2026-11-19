//! Paid selected-value transforms; no World projection or source permission.
use super::*;
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    io::{self, Write},
};
pub(in crate::world) struct OwnedJson {
    pub(in crate::world) value: serde_json::Value,
    _fee: oasis7_proto::memory_budget::MemoryReservation,
}
pub(in crate::world) struct OwnedContinuations {
    pub(in crate::world) value: Vec<crate::cognition_wake::AgentContinuation>,
    _fee: oasis7_proto::memory_budget::MemoryReservation,
}
fn limit() -> BodyMutationError {
    ValueTreeError::ResourceLimited("Cognition expiry value layout").into()
}
struct JsonCount<'a, 'b> {
    common: &'a mut FormalBoundedContextV1<'b>,
    bytes: usize,
    error: Option<oasis7_formal_protocol::controlled_authority::formal::FormalError>,
}
impl Write for JsonCount<'_, '_> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.error.is_some() {
            return Err(io::ErrorKind::Other.into());
        }
        if let Err(e) = self.common.charge(1, b.len() as u64) {
            self.error = Some(e);
            return Err(io::ErrorKind::Other.into());
        }
        self.bytes = self
            .bytes
            .checked_add(b.len())
            .ok_or(io::ErrorKind::InvalidData)?;
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
/// Count the actual selected serializer before allocating its JSON graph.
/// One full JSON BTree internal-node bound per encoded byte covers all node
/// and String payloads, including tiny empty children and serde growth peaks.
pub(in crate::world) fn json<T: Serialize + ?Sized>(
    value: &T,
    max_heap: usize,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<OwnedJson, BodyMutationError> {
    let mut count = JsonCount {
        common,
        bytes: 0,
        error: None,
    };
    let encoded = serde_json::to_writer(&mut count, value);
    if let Some(e) = count.error.take() {
        return Err(BodyMutationError::Original(Box::new(e)));
    }
    encoded.map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let n = count.bytes;
    let heap = n
        .checked_mul(2048)
        .and_then(|n| n.checked_add(4096))
        .ok_or_else(limit)?;
    if heap > max_heap {
        return Err(limit());
    }
    count.common.charge(n as u64, n as u64).map_err(formal)?;
    let fee = count
        .common
        .memory()
        .reserve(heap, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let value =
        serde_json::to_value(value).map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    Ok(OwnedJson { value, _fee: fee })
}
/// The same actual graph admission covers borrowed JSON Deserialize's typed
/// continuation graph and internally tagged wake-condition Content peak.
pub(in crate::world) fn continuations(
    value: &serde_json::Value,
    max_heap: usize,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<OwnedContinuations, BodyMutationError> {
    let mut count = JsonCount {
        common,
        bytes: 0,
        error: None,
    };
    let encoded = serde_json::to_writer(&mut count, value);
    if let Some(e) = count.error.take() {
        return Err(formal(e));
    }
    encoded.map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let n = count.bytes;
    let heap = n
        .checked_mul(2048)
        .and_then(|n| n.checked_add(4096))
        .ok_or_else(limit)?;
    if heap > max_heap {
        return Err(limit());
    }
    count.common.charge(n as u64, n as u64).map_err(formal)?;
    let fee = count
        .common
        .memory()
        .reserve(heap, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let parsed = Vec::<crate::cognition_wake::AgentContinuation>::deserialize(value)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    Ok(OwnedContinuations {
        value: parsed,
        _fee: fee,
    })
}
pub(in crate::world) fn digest<T: Serialize + ?Sized>(
    domain: &str,
    value: &T,
    work: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<String>, BodyMutationError> {
    use oasis7_wasm_abi::{
        CanonicalCborAllocator, CanonicalCborStreamContextV1, CanonicalCborStreamError as Error,
        stream_canonical_cbor_v1,
    };
    enum Typed {
        Common(oasis7_formal_protocol::controlled_authority::formal::FormalError),
        Memory(oasis7_proto::memory_budget::MemoryLimitExceeded),
    }
    struct Meter<'a, 'b> {
        common: RefCell<&'a mut FormalBoundedContextV1<'b>>,
        memory: oasis7_proto::memory_budget::MemoryBudget,
        error: RefCell<Option<Typed>>,
    }
    impl CanonicalCborAllocator for Meter<'_, '_> {
        type Charge = oasis7_proto::memory_budget::MemoryReservation;
        fn reserve(&self, n: usize) -> Result<Self::Charge, Error> {
            if self.error.borrow().is_some() {
                return Err(Error::Poisoned);
            }
            self.memory.reserve(n, MemoryClass::Ordinary).map_err(|e| {
                *self.error.borrow_mut() = Some(Typed::Memory(e));
                Error::ResourceLimited("expiry canonical heap")
            })
        }
        fn charge_work(&self, s: u64, b: u64) -> Result<(), Error> {
            if self.error.borrow().is_some() {
                return Err(Error::Poisoned);
            }
            self.common.borrow_mut().charge(s, b).map_err(|e| {
                *self.error.borrow_mut() = Some(Typed::Common(e));
                Error::ResourceLimited("expiry canonical Common")
            })
        }
    }
    struct Hash<'a, 'b, 'c> {
        meter: &'a Meter<'b, 'c>,
        hash: blake3::Hasher,
    }
    impl Write for Hash<'_, '_, '_> {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.meter
                .charge_work(1, b.len() as u64)
                .map_err(|_| io::ErrorKind::Other)?;
            self.hash.update(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let fee = common
        .memory()
        .reserve(71 + std::mem::size_of::<String>(), MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let meter = Meter {
        memory: common.memory().clone(),
        common: RefCell::new(common),
        error: RefCell::new(None),
    };
    let empty = oasis7_wasm_abi::CanonicalCborWorkV1 {
        visits_remaining: 0,
        bytes_remaining: 0,
        max_depth: work.max_depth,
        max_map_keys: work.max_map_keys,
        max_key_bytes: work.max_key_bytes,
    };
    let mut context =
        CanonicalCborStreamContextV1::new(&meter, std::mem::replace(work, empty), || false);
    let mut output = Hash {
        meter: &meter,
        hash: blake3::Hasher::new(),
    };
    let result = stream_canonical_cbor_v1(&(domain, value), &mut context, &mut output);
    *work = context.work;
    if let Some(e) = meter.error.borrow_mut().take() {
        return Err(match e {
            Typed::Common(e) => formal(e),
            Typed::Memory(e) => ValueTreeError::Memory(e).into(),
        });
    }
    result.map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    meter.common.borrow_mut().charge(1, 71).map_err(formal)?;
    let mut digest = String::with_capacity(71);
    digest.push_str("blake3:");
    digest.push_str(output.hash.finalize().to_hex().as_str());
    Ok(fee.retain(digest))
}
