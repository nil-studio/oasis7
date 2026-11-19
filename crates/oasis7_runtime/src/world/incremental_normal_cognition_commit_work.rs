//! Same caller Common and canonical lane; no new budget, refund or allowance.
use super::*;
use oasis7_wasm_abi::{
    CanonicalCborAllocator, CanonicalCborStreamError as Error, CapabilityCanonicalWorkV1,
};
pub(super) struct Source<'a, 'b, 'c, 'k> {
    pub(super) common: &'a RefCell<&'b mut FormalBoundedContextV1<'c>>,
    canonical: RefCell<&'k mut oasis7_wasm_abi::CanonicalCborWorkV1>,
    first: RefCell<Option<BodyMutationError>>,
}
impl<'a, 'b, 'c, 'k> Source<'a, 'b, 'c, 'k> {
    pub(super) fn new(
        common: &'a RefCell<&'b mut FormalBoundedContextV1<'c>>,
        canonical: &'k mut oasis7_wasm_abi::CanonicalCborWorkV1,
    ) -> Self {
        Self {
            common,
            canonical: RefCell::new(canonical),
            first: RefCell::new(None),
        }
    }
    pub(super) fn result<T>(&self, value: Result<T, Error>) -> Result<T, BodyMutationError> {
        if let Some(first) = self.first.borrow_mut().take() {
            return Err(first);
        }
        value.map_err(original)
    }
    pub(super) fn refusal(
        &self,
        code: &str,
    ) -> Result<CurrentCognitionCommitErrorV1, BodyMutationError> {
        let n = code
            .len()
            .checked_mul(2)
            .and_then(|n| n.checked_add(size_of::<PaidCurrentCognitionCommitRefusalV1>() + 4096))
            .ok_or_else(|| bad("Cognition refusal layout"))?;
        self.result(self.charge_work(1, n as u64))?;
        let fee = self
            .common
            .borrow()
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let error = crate::world::cognition_persistence_validation::cognition_validation(code);
        Ok(CurrentCognitionCommitErrorV1::Business(
            PaidCurrentCognitionCommitRefusalV1 { error, _fee: fee },
        ))
    }
}
impl CanonicalCborAllocator for Source<'_, '_, '_, '_> {
    type Charge = MemoryReservation;
    fn charge_work(&self, s: u64, b: u64) -> Result<(), Error> {
        if self.first.borrow().is_some() {
            return Err(Error::Poisoned);
        }
        self.common.borrow_mut().charge(s, b).map_err(|e| {
            *self.first.borrow_mut() = Some(original(e));
            Error::Poisoned
        })
    }
    fn reserve(&self, n: usize) -> Result<MemoryReservation, Error> {
        if self.first.borrow().is_some() {
            return Err(Error::Poisoned);
        }
        self.common
            .borrow()
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(|e| {
                *self.first.borrow_mut() = Some(ValueTreeError::Memory(e).into());
                Error::Poisoned
            })
    }
}
impl CapabilityCanonicalWorkV1 for Source<'_, '_, '_, '_> {
    type Loan<'x>
        = std::cell::RefMut<'x, oasis7_wasm_abi::CanonicalCborWorkV1>
    where
        Self: 'x;
    fn is_metered(&self) -> bool {
        true
    }
    fn loan_canonical_work(&self) -> Self::Loan<'_> {
        std::cell::RefMut::map(self.canonical.borrow_mut(), |w| &mut **w)
    }
    fn record_stream_error(&self, error: Error) -> Error {
        if self.first.borrow().is_none() {
            *self.first.borrow_mut() = Some(original(error))
        }
        Error::Poisoned
    }
}
