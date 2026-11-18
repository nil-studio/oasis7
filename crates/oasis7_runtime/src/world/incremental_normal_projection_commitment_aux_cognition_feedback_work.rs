//! Actual continuing Common/canonical source. No allowance is copied or reset.
use super::*;
use oasis7_wasm_abi::{
    CanonicalCborAllocator, CanonicalCborStreamError as Error, CapabilityCanonicalWorkV1,
};
pub(super) struct Source<'v, 'f, 'a, 'c, 'k> {
    view: &'v values::View<'f, 'a, 'c>,
    canonical: RefCell<&'k mut oasis7_wasm_abi::CanonicalCborWorkV1>,
    first: RefCell<Option<BodyMutationError>>,
}
impl<'v, 'f, 'a, 'c, 'k> Source<'v, 'f, 'a, 'c, 'k> {
    pub(super) fn new(
        view: &'v values::View<'f, 'a, 'c>,
        canonical: &'k mut oasis7_wasm_abi::CanonicalCborWorkV1,
    ) -> Self {
        Self {
            view,
            canonical: RefCell::new(canonical),
            first: RefCell::new(None),
        }
    }
    pub(super) fn check(&self) -> Result<(), BodyMutationError> {
        if let Some(error) = self.first.borrow_mut().take() {
            return Err(error);
        }
        self.view.facts.check()
    }
    pub(super) fn receipt<T>(
        &self,
        value: Result<T, ReceiptReadErrorV1>,
    ) -> Result<T, BodyMutationError> {
        self.check()?;
        match value {
            Ok(v) => Ok(v),
            Err(ReceiptReadErrorV1::World(error)) => {
                Err(self.view.business(format_args!("{error:?}")))
            }
            Err(ReceiptReadErrorV1::Work(error)) => Err(paid(error)),
        }
    }
}
impl CanonicalCborAllocator for Source<'_, '_, '_, '_, '_> {
    type Charge = MemoryReservation;
    fn charge_work(&self, s: u64, b: u64) -> Result<(), Error> {
        if self.first.borrow().is_some() {
            return Err(Error::Poisoned);
        }
        self.view.facts.work(s, b as usize).map_err(|e| {
            *self.first.borrow_mut() = Some(e);
            Error::Poisoned
        })
    }
    fn reserve(&self, n: usize) -> Result<MemoryReservation, Error> {
        if self.first.borrow().is_some() {
            return Err(Error::Poisoned);
        }
        self.view
            .facts
            .common
            .borrow()
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(|e| {
                *self.first.borrow_mut() = Some(ValueTreeError::Memory(e).into());
                Error::Poisoned
            })
    }
}
impl CapabilityCanonicalWorkV1 for Source<'_, '_, '_, '_, '_> {
    type Loan<'x>
        = std::cell::RefMut<'x, oasis7_wasm_abi::CanonicalCborWorkV1>
    where
        Self: 'x;
    fn is_metered(&self) -> bool {
        true
    }
    fn loan_canonical_work(&self) -> Self::Loan<'_> {
        std::cell::RefMut::map(self.canonical.borrow_mut(), |work| &mut **work)
    }
    fn record_stream_error(&self, error: Error) -> Error {
        if self.first.borrow().is_none() {
            *self.first.borrow_mut() = Some(paid(error))
        }
        Error::Poisoned
    }
}
