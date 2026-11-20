//! The original cognition digest preimage; canonical encoding and hashing
//! belong to the shared domain kernel. This contains digest DATA only.
use super::*;
pub use oasis7_domain_types::canonical_digest::{
    PaidCanonicalBlake3DataV1 as PaidCognitionDigestDataV1, canonical_blake3_data_with_work_v1,
};
use oasis7_wasm_abi::{CanonicalCborStreamError as Error, CapabilityCanonicalWorkV1};

pub fn cognition_digest_with_work_v1<T: Serialize + ?Sized, A: CapabilityCanonicalWorkV1>(
    domain: &str,
    payload: &T,
    source: &A,
) -> Result<PaidCognitionDigestDataV1<A::Charge>, Error> {
    canonical_blake3_data_with_work_v1(&cognition_digest_input_v1(domain, payload), source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;
    struct TestPool(Rc<Cell<usize>>);
    struct TestCharge {
        pool: Rc<Cell<usize>>,
        bytes: usize,
    }
    impl Drop for TestCharge {
        fn drop(&mut self) {
            self.pool.set(self.pool.get() - self.bytes);
        }
    }
    impl TestPool {
        fn used(&self) -> usize {
            self.0.get()
        }
        fn reserve(&self, bytes: usize) -> Result<TestCharge, Error> {
            let next = self
                .0
                .get()
                .checked_add(bytes)
                .filter(|n| *n <= 8 << 20)
                .ok_or(Error::ResourceLimited("digest test memory"))?;
            self.0.set(next);
            Ok(TestCharge {
                pool: Rc::clone(&self.0),
                bytes,
            })
        }
    }
    use oasis7_wasm_abi::{CanonicalCborAllocator, CanonicalCborWorkV1};
    use std::cell::{Cell, RefCell, RefMut};
    struct Meter {
        memory: TestPool,
        work: RefCell<CanonicalCborWorkV1>,
        steps: Cell<u64>,
        cancelled: Cell<bool>,
    }
    impl Meter {
        fn new(steps: u64) -> Self {
            Self {
                memory: TestPool(Rc::new(Cell::new(0))),
                work: RefCell::new(CanonicalCborWorkV1 {
                    visits_remaining: 100_000,
                    bytes_remaining: 1 << 20,
                    max_depth: 64,
                    max_map_keys: 1024,
                    max_key_bytes: 1 << 16,
                }),
                steps: Cell::new(steps),
                cancelled: Cell::new(false),
            }
        }
    }
    impl CanonicalCborAllocator for Meter {
        type Charge = TestCharge;
        fn reserve(&self, n: usize) -> Result<Self::Charge, Error> {
            self.memory
                .reserve(n)
                .map_err(|_| Error::ResourceLimited("digest test memory"))
        }
        fn charge_work(&self, s: u64, _: u64) -> Result<(), Error> {
            if self.cancelled.get() {
                return Err(Error::Cancelled);
            }
            self.steps.set(
                self.steps
                    .get()
                    .checked_sub(s)
                    .ok_or(Error::ResourceLimited("digest test steps"))?,
            );
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
    }
    #[test]
    fn paid_digest_keeps_original_cognition_and_world_service_preimages() {
        let meter = Meter::new(100_000);
        let raw = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let domain = "oasis7.runtime.manifest.v1";
        let expected = format!(
            "blake3:{}",
            blake3::hash(&oasis7_wasm_abi::encode_canonical_cbor(&(domain, raw)).unwrap())
        );
        assert_eq!(cognition_digest_v1(domain, &raw), expected);
        let output = cognition_digest_with_work_v1(domain, &raw, &meter).unwrap();
        assert_eq!(output.as_str(), expected);
        assert!(meter.memory.used() > 0);
        drop(output);
        assert_eq!(meter.memory.used(), 0);
        let value = (2u64, "中文\0原文");
        let input = (
            "oasis7.world-service.v1",
            "formal-projection-root-v1",
            &value,
        );
        let expected = format!(
            "blake3:{}",
            blake3::hash(&oasis7_wasm_abi::encode_canonical_cbor(&input).unwrap())
        );
        let output = canonical_blake3_data_with_work_v1(&input, &meter).unwrap();
        assert_eq!(output.as_str(), expected);
        assert_eq!(output.as_str().len(), 71);
        assert!(meter.work.borrow().visits_remaining < 100_000);
        drop(output);
        assert_eq!(meter.memory.used(), 0);
    }
    #[test]
    fn work_refusal_and_cancel_do_not_refund_or_leave_digest_fees() {
        let meter = Meter::new(0);
        assert!(matches!(
            cognition_digest_with_work_v1("domain", &"payload", &meter),
            Err(Error::ResourceLimited("digest test steps"))
        ));
        assert_eq!(meter.memory.used(), 0);
        assert_eq!(meter.steps.get(), 0);
        assert_eq!(meter.work.borrow().visits_remaining, 100_000);
        let meter = Meter::new(100_000);
        meter.cancelled.set(true);
        assert!(matches!(
            cognition_digest_with_work_v1("domain", &"payload", &meter),
            Err(Error::Cancelled)
        ));
        assert_eq!(meter.memory.used(), 0);
        assert_eq!(meter.work.borrow().visits_remaining, 100_000);
    }
}
