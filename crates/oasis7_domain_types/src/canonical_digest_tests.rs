use super::*;
use oasis7_wasm_abi::{CanonicalCborWorkV1, LegacyCapabilityCanonicalWorkV1};
use std::{
    cell::{Cell, RefCell, RefMut},
    rc::Rc,
};

struct Charge(Rc<Cell<usize>>, usize);
impl Drop for Charge {
    fn drop(&mut self) {
        self.0.set(self.0.get() - self.1);
    }
}
struct Meter {
    work: RefCell<CanonicalCborWorkV1>,
    used: Rc<Cell<usize>>,
    calls: Cell<usize>,
    fail_at: usize,
    cancel: bool,
    recorded: RefCell<Vec<String>>,
}
impl Meter {
    fn new(fail_at: usize, cancel: bool) -> Self {
        Self {
            work: RefCell::new(CanonicalCborWorkV1 {
                visits_remaining: 100_000,
                bytes_remaining: 1 << 20,
                max_depth: 64,
                max_map_keys: 1024,
                max_key_bytes: 1 << 16,
            }),
            used: Rc::new(Cell::new(0)),
            calls: Cell::new(0),
            fail_at,
            cancel,
            recorded: RefCell::new(Vec::new()),
        }
    }
}
impl CanonicalCborAllocator for Meter {
    type Charge = Charge;
    fn reserve(&self, n: usize) -> Result<Charge, Error> {
        let used = self
            .used
            .get()
            .checked_add(n)
            .filter(|n| *n <= 8 << 20)
            .ok_or(Error::ResourceLimited("digest memory"))?;
        self.used.set(used);
        Ok(Charge(Rc::clone(&self.used), n))
    }
    fn charge_work(&self, _: u64, _: u64) -> Result<(), Error> {
        let call = self.calls.get() + 1;
        self.calls.set(call);
        if call >= self.fail_at {
            return Err(if self.cancel {
                Error::Cancelled
            } else {
                Error::ResourceLimited("original digest work refusal")
            });
        }
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
    fn record_stream_error(&self, error: Error) -> Error {
        self.recorded.borrow_mut().push(error.to_string());
        error
    }
}

#[test]
fn paid_and_legacy_digest_match_independent_canonical_bytes() {
    let value = serde_json::json!({"z": [true, null, "中\u{0}文"], "a": 17});
    let preimage = (
        "oasis7.world-service.v1",
        "formal-projection-leaf-v1",
        value,
    );
    let expected = format!(
        "blake3:{}",
        blake3::hash(&oasis7_wasm_abi::encode_canonical_cbor(&preimage).unwrap())
    );
    let meter = Meter::new(usize::MAX, false);
    let first = canonical_blake3_data_with_work_v1(&preimage, &meter).unwrap();
    let remaining = meter.work.borrow().visits_remaining;
    let second = canonical_blake3_data_with_work_v1(&preimage, &meter).unwrap();
    let legacy =
        canonical_blake3_data_with_work_v1(&preimage, &LegacyCapabilityCanonicalWorkV1).unwrap();
    assert_eq!(first.as_str(), expected);
    assert_eq!(second.as_str(), expected);
    assert_eq!(legacy.as_str(), expected);
    assert!(remaining < 100_000);
    assert!(meter.work.borrow().visits_remaining < remaining);
    assert!(
        meter.used.get() >= 2 * (std::mem::size_of::<PaidCanonicalBlake3DataV1<Charge>>() + 71)
    );
    drop(first);
    assert!(meter.used.get() > 0);
    drop(second);
    assert_eq!(meter.used.get(), 0);
    assert!(meter.recorded.borrow().is_empty());
}

#[test]
fn every_digest_debit_preserves_typed_first_failure_and_releases_allocations() {
    let value = ("preimage", vec!["Unicode\u{0}中文", "payload"]);
    let complete = Meter::new(usize::MAX, false);
    drop(canonical_blake3_data_with_work_v1(&value, &complete).unwrap());
    let calls = complete.calls.get();
    assert!(calls > 5);
    for cancel in [false, true] {
        for fail_at in 1..=calls {
            let meter = Meter::new(fail_at, cancel);
            let error = match canonical_blake3_data_with_work_v1(&value, &meter) {
                Ok(_) => panic!("debit {fail_at} unexpectedly succeeded"),
                Err(error) => error,
            };
            if cancel {
                assert!(
                    matches!(error, Error::Cancelled),
                    "debit {fail_at}: {error:?}"
                );
            } else {
                assert!(
                    matches!(
                        error,
                        Error::ResourceLimited("original digest work refusal")
                    ),
                    "debit {fail_at}: {error:?}"
                );
            }
            assert_eq!(
                meter.calls.get(),
                fail_at,
                "work continued after first failure"
            );
            assert_eq!(
                meter.used.get(),
                0,
                "debit {fail_at} leaked its reservation"
            );
            let recorded = meter.recorded.borrow();
            assert!(
                recorded.len() <= 1,
                "stream hook recorded the same failure twice"
            );
            if let Some(recorded) = recorded.first() {
                assert_eq!(recorded, &error.to_string());
            }
        }
    }
}
