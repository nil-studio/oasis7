//! Paid DATA hashing. Work sources can supply only allocation and continuing
//! counters; this module alone chooses the canonical encoder and SHA256 output.
use super::{CapabilityAuthorizationValidationError, sha256_stream::Sha256V1};
use crate::{
    CanonicalCborAllocator, CanonicalCborStreamContextV1, CanonicalCborStreamError,
    CanonicalCborWorkV1, stream_canonical_cbor_v1,
};
use serde::Serialize;
use std::{
    io::{self, Write},
    ops::DerefMut,
};
pub trait CapabilityCanonicalWorkV1: CanonicalCborAllocator {
    type Loan<'a>: DerefMut<Target = CanonicalCborWorkV1>
    where
        Self: 'a;
    fn is_metered(&self) -> bool;
    fn loan_canonical_work(&self) -> Self::Loan<'_>;
    /// Preserve the original stream failure in a continuing typed host meter.
    /// This hook cannot provide data, a digest or a business verdict.
    fn record_stream_error(&self, error: CanonicalCborStreamError) -> CanonicalCborStreamError {
        error
    }
}
pub struct CanonicalSha256DataV1<C> {
    text: String,
    _fee: Option<C>,
}
impl<C> CanonicalSha256DataV1<C> {
    pub fn as_str(&self) -> &str {
        &self.text
    }
}
#[derive(Debug)]
pub enum CapabilityCanonicalWorkErrorV1 {
    Legacy(CapabilityAuthorizationValidationError),
    Stream(CanonicalCborStreamError),
}
impl std::fmt::Display for CapabilityCanonicalWorkErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Legacy(e) => e.fmt(f),
            Self::Stream(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for CapabilityCanonicalWorkErrorV1 {}
impl From<CanonicalCborStreamError> for CapabilityCanonicalWorkErrorV1 {
    fn from(e: CanonicalCborStreamError) -> Self {
        Self::Stream(e)
    }
}
/// Legacy and paid DATA share the exact preimage. The legacy branch keeps its
/// original codec/error order; metered execution uses the existing canonical
/// streaming encoder, with no intermediate DOM or encoded Vec.
pub fn canonical_hash_with_work_v1<T: Serialize + ?Sized, A: CapabilityCanonicalWorkV1>(
    value: &T,
    source: &A,
) -> Result<CanonicalSha256DataV1<A::Charge>, CapabilityCanonicalWorkErrorV1> {
    if !source.is_metered() {
        return Ok(CanonicalSha256DataV1 {
            text: super::canonical_hash(&value).map_err(CapabilityCanonicalWorkErrorV1::Legacy)?,
            _fee: None,
        });
    }
    source.charge_work(
        1,
        (std::mem::size_of::<CanonicalSha256DataV1<A::Charge>>() + 64) as u64,
    )?;
    let fee = source.reserve(std::mem::size_of::<CanonicalSha256DataV1<A::Charge>>() + 64)?;
    struct Hash<'a, A: CanonicalCborAllocator> {
        source: &'a A,
        hash: Sha256V1,
        first_error: Option<CanonicalCborStreamError>,
    }
    impl<A: CanonicalCborAllocator> Write for Hash<'_, A> {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            if let Err(error) = self
                .hash
                .update(b, &mut |s, b| self.source.charge_work(s, b))
            {
                self.first_error.get_or_insert(error);
                return Err(io::ErrorKind::Other.into());
            }
            Ok(b.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut sink = Hash {
        source,
        hash: Sha256V1::new(),
        first_error: None,
    };
    let result = stream_canonical_cbor_loan_v1(value, source, &mut sink);
    if let Some(error) = sink.first_error {
        return Err(source.record_stream_error(error).into());
    }
    result.map_err(|error| source.record_stream_error(error))?;
    let digest = sink.hash.finish(&mut |s, b| source.charge_work(s, b))?;
    source.charge_work(1, 64)?;
    let mut text = String::with_capacity(64);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for b in digest {
        text.push(HEX[(b >> 4) as usize] as char);
        text.push(HEX[(b & 15) as usize] as char)
    }
    source.charge_work(0, 0)?;
    Ok(CanonicalSha256DataV1 {
        text,
        _fee: Some(fee),
    })
}
/// The original wrappers have no meter or permission. The loan is deliberately
/// unreachable rather than inventing a new bounded allowance for old callers.
pub struct LegacyCapabilityCanonicalWorkV1;
impl CanonicalCborAllocator for LegacyCapabilityCanonicalWorkV1 {
    type Charge = ();
    fn reserve(&self, _: usize) -> Result<(), CanonicalCborStreamError> {
        Ok(())
    }
}
impl CapabilityCanonicalWorkV1 for LegacyCapabilityCanonicalWorkV1 {
    type Loan<'a> = std::cell::RefMut<'a, CanonicalCborWorkV1>;
    fn is_metered(&self) -> bool {
        false
    }
    fn loan_canonical_work(&self) -> Self::Loan<'_> {
        unreachable!("legacy canonical work has no loan")
    }
}

/// Transfer the caller's actual continuing codec allowance for one stream.
/// The original counters are restored by actual delta, also during unwinding.
pub fn stream_canonical_cbor_with_work_v1<T: Serialize + ?Sized, A: CapabilityCanonicalWorkV1>(
    value: &T,
    source: &A,
    output: &mut dyn Write,
) -> Result<(), CanonicalCborStreamError> {
    stream_canonical_cbor_loan_v1(value, source, output)
        .map_err(|error| source.record_stream_error(error))
}

// Keep the original SHA refusal until the IO adapter can restore its typed cause.
// Record exactly one stream error after returning the continuing codec allowance.
fn stream_canonical_cbor_loan_v1<T: Serialize + ?Sized, A: CapabilityCanonicalWorkV1>(
    value: &T,
    source: &A,
    output: &mut dyn Write,
) -> Result<(), CanonicalCborStreamError> {
    let mut loan = source.loan_canonical_work();
    let moved = CanonicalCborWorkV1 {
        visits_remaining: loan.visits_remaining,
        bytes_remaining: loan.bytes_remaining,
        max_depth: loan.max_depth,
        max_map_keys: loan.max_map_keys,
        max_key_bytes: loan.max_key_bytes,
    };
    loan.visits_remaining = 0;
    loan.bytes_remaining = 0;
    struct Restore<'a, A: CanonicalCborAllocator> {
        original: &'a mut CanonicalCborWorkV1,
        context: CanonicalCborStreamContextV1<'a, A, fn() -> bool>,
    }
    impl<A: CanonicalCborAllocator> Drop for Restore<'_, A> {
        fn drop(&mut self) {
            self.original.visits_remaining = self.context.work.visits_remaining;
            self.original.bytes_remaining = self.context.work.bytes_remaining
        }
    }
    let mut active = Restore {
        original: &mut loan,
        context: CanonicalCborStreamContextV1::new(source, moved, (|| false) as fn() -> bool),
    };
    stream_canonical_cbor_v1(value, &mut active.context, output).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell, RefMut};
    use std::rc::Rc;
    struct Charge {
        live: Rc<Cell<usize>>,
        bytes: usize,
    }
    impl Drop for Charge {
        fn drop(&mut self) {
            self.live.set(self.live.get() - self.bytes);
        }
    }
    struct Meter {
        work: RefCell<CanonicalCborWorkV1>,
        steps: Cell<u64>,
        live: Rc<Cell<usize>>,
        reports: Cell<usize>,
        last_error: Cell<&'static str>,
    }
    impl Meter {
        fn new(steps: u64) -> Self {
            Self {
                work: RefCell::new(CanonicalCborWorkV1 {
                    visits_remaining: 1_000_000,
                    bytes_remaining: 1 << 24,
                    max_depth: 64,
                    max_map_keys: 1000,
                    max_key_bytes: 1 << 20,
                }),
                steps: Cell::new(steps),
                live: Rc::new(Cell::new(0)),
                reports: Cell::new(0),
                last_error: Cell::new("none"),
            }
        }
    }
    impl CanonicalCborAllocator for Meter {
        type Charge = Charge;
        fn reserve(&self, bytes: usize) -> Result<Charge, CanonicalCborStreamError> {
            self.live.set(self.live.get() + bytes);
            Ok(Charge {
                live: Rc::clone(&self.live),
                bytes,
            })
        }
        fn charge_work(&self, s: u64, _: u64) -> Result<(), CanonicalCborStreamError> {
            let n = self.steps.get().checked_sub(s).ok_or(
                CanonicalCborStreamError::ResourceLimited("test actual steps"),
            )?;
            self.steps.set(n);
            Ok(())
        }
    }
    impl CapabilityCanonicalWorkV1 for Meter {
        type Loan<'a> = RefMut<'a, CanonicalCborWorkV1>;
        fn is_metered(&self) -> bool {
            true
        }
        fn loan_canonical_work(&self) -> Self::Loan<'_> {
            self.work.borrow_mut()
        }
        fn record_stream_error(&self, error: CanonicalCborStreamError) -> CanonicalCborStreamError {
            self.reports.set(self.reports.get() + 1);
            self.last_error.set(match &error {
                CanonicalCborStreamError::ResourceLimited(reason) => reason,
                CanonicalCborStreamError::Io(_) => "io",
                _ => "other",
            });
            error
        }
    }
    #[test]
    fn original_sha_rounds_keep_standard_vector() {
        assert_eq!(
            super::super::canonical_sha256_hex(b"abc").unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
    #[test]
    fn paid_canonical_sha_keeps_original_unicode_and_block_boundary_wire() {
        let value = serde_json::json!({"中文":[true,7,"x".repeat(8193)],"a":{"z":null,"é":"文本"}});
        let meter = Meter::new(1_000_000);
        let hash = canonical_hash_with_work_v1(&value, &meter).unwrap();
        assert_eq!(hash.as_str(), super::super::canonical_hash(&value).unwrap());
        assert!(meter.work.borrow().visits_remaining < 1_000_000);
        assert!(meter.steps.get() < 1_000_000);
        assert!(meter.live.get() >= 64);
        drop(hash);
        assert_eq!(meter.live.get(), 0);
    }
    #[test]
    fn failed_stream_returns_only_actual_codec_allowance() {
        // Hash-header admission consumes one host step. Engine::encode then
        // charges the host BEFORE its first codec visit. One remaining host
        // step reaches encode, while the next emit fails before its debit.
        for (steps, spent_visits) in [(1, 0), (2, 1)] {
            let meter = Meter::new(steps);
            let before = meter.work.borrow().visits_remaining;
            let before_bytes = meter.work.borrow().bytes_remaining;
            let error = match canonical_hash_with_work_v1(&vec!["x"; 3], &meter) {
                Err(error) => error,
                Ok(_) => panic!("missing actual host-work refusal"),
            };
            assert!(matches!(
                error,
                CapabilityCanonicalWorkErrorV1::Stream(CanonicalCborStreamError::ResourceLimited(
                    "test actual steps"
                ))
            ));
            assert_eq!(meter.steps.get(), 0);
            assert_eq!(meter.work.borrow().visits_remaining, before - spent_visits);
            assert_eq!(meter.work.borrow().bytes_remaining, before_bytes);
            assert_eq!(meter.live.get(), 0);
        }
    }

    #[test]
    fn sha_sink_refusal_preserves_first_typed_error_and_records_it_once() {
        // Header admission, encoding and output admission consume four steps.
        // The next step belongs to the SHA writer's actual input copy.
        let meter = Meter::new(4);
        let error = match canonical_hash_with_work_v1(&7_u8, &meter) {
            Err(error) => error,
            Ok(_) => panic!("missing SHA copy work refusal"),
        };
        assert!(matches!(
            error,
            CapabilityCanonicalWorkErrorV1::Stream(CanonicalCborStreamError::ResourceLimited(
                "test actual steps"
            ))
        ));
        assert_eq!(meter.reports.get(), 1);
        assert_eq!(meter.last_error.get(), "test actual steps");
        assert_eq!(meter.live.get(), 0);
    }

    #[test]
    fn repeated_streams_use_the_remaining_allowance() {
        let meter = Meter::new(1_000_000);
        let value = serde_json::json!({"a": [1, 2, 3]});
        let mut first = Vec::new();
        stream_canonical_cbor_with_work_v1(&value, &meter, &mut first).unwrap();
        let visits = meter.work.borrow().visits_remaining;
        let bytes = meter.work.borrow().bytes_remaining;
        let mut second = Vec::new();
        stream_canonical_cbor_with_work_v1(&value, &meter, &mut second).unwrap();
        assert_eq!(first, second);
        assert!(meter.work.borrow().visits_remaining < visits);
        assert!(meter.work.borrow().bytes_remaining < bytes);
        assert_eq!(meter.live.get(), 0);
    }

    #[test]
    fn output_failure_returns_allowance_without_refunding_spent_work() {
        struct Refused;
        impl Write for Refused {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::PermissionDenied.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let meter = Meter::new(1_000_000);
        let error = stream_canonical_cbor_with_work_v1(&7_u8, &meter, &mut Refused).unwrap_err();
        assert!(matches!(error, CanonicalCborStreamError::Io(error)
            if error.kind() == io::ErrorKind::PermissionDenied));
        assert!(meter.work.borrow().visits_remaining < 1_000_000);
        assert!(meter.work.borrow().bytes_remaining < 1 << 24);
        assert_eq!(meter.reports.get(), 1);
        assert_eq!(meter.live.get(), 0);
    }

    #[test]
    fn unwinding_restores_the_actual_remaining_allowance() {
        struct Panicking;
        impl Serialize for Panicking {
            fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
                panic!("serializer stopped");
            }
        }
        let meter = Meter::new(1_000_000);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            stream_canonical_cbor_with_work_v1(&Panicking, &meter, &mut Vec::new())
        }));
        assert!(result.is_err());
        assert!(meter.work.borrow().visits_remaining > 0);
        assert!(meter.work.borrow().visits_remaining < 1_000_000);
        assert_eq!(meter.work.borrow().bytes_remaining, 1 << 24);
        assert_eq!(meter.live.get(), 0);
    }

    #[test]
    fn legacy_hash_uses_the_original_encoder_without_a_meter_loan() {
        let value = serde_json::json!({"a": ["原件", 7]});
        let hash = canonical_hash_with_work_v1(&value, &LegacyCapabilityCanonicalWorkV1).unwrap();
        assert_eq!(hash.as_str(), super::super::canonical_hash(&value).unwrap());
    }
}
