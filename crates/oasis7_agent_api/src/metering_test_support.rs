//! Fault injection for the real canonical-stream error hook and continuing loan.
use oasis7_wasm_abi::{
    CanonicalCborAllocator, CanonicalCborStreamError as Error, CanonicalCborWorkV1,
    CapabilityCanonicalWorkV1,
};
use std::cell::{Cell, RefCell, RefMut};

pub(crate) struct Meter {
    work: RefCell<CanonicalCborWorkV1>,
    pub(crate) calls: Cell<usize>,
    fail_at: usize,
    cancel: bool,
    pub(crate) recorded: RefCell<Vec<String>>,
}
impl Meter {
    pub(crate) fn new(fail_at: usize, cancel: bool) -> Self {
        Self {
            work: RefCell::new(CanonicalCborWorkV1 {
                visits_remaining: 100_000,
                bytes_remaining: 1 << 20,
                max_depth: 64,
                max_map_keys: 1024,
                max_key_bytes: 4096,
            }),
            calls: Cell::new(0),
            fail_at,
            cancel,
            recorded: RefCell::new(Vec::new()),
        }
    }
    pub(crate) fn failure(&self) -> Error {
        if self.cancel {
            Error::Cancelled
        } else {
            Error::ResourceLimited("original API work refusal")
        }
    }
    pub(crate) fn assert_first_cause(&self) {
        assert_eq!(
            self.calls.get(),
            self.fail_at,
            "work continued after failure"
        );
        let recorded = self.recorded.borrow();
        assert!(recorded.len() <= 1, "failure was recorded more than once");
        if let Some(error) = recorded.first() {
            assert_eq!(error, &self.failure().to_string());
        }
    }
}
impl CanonicalCborAllocator for Meter {
    type Charge = ();
    fn reserve(&self, _: usize) -> Result<(), Error> {
        Ok(())
    }
    fn charge_work(&self, _: u64, _: u64) -> Result<(), Error> {
        let call = self.calls.get() + 1;
        self.calls.set(call);
        if call >= self.fail_at {
            Err(self.failure())
        } else {
            Ok(())
        }
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
