//! Work-only hooks for the original economy predicates and hash wire. A hook
//! cannot supply a business match, request, receipt, or alternate digest domain.
use super::*;
pub(in crate::world) trait CognitionEconomyWorkOpsV1:
    sealed::Sealed + oasis7_wasm_abi::CapabilityCanonicalWorkV1
{
    fn work(&mut self, steps: u64, bytes: usize) -> Result<(), CognitionEconomyError>;
    /// Admit the following original owned clone/JSON intermediate before it
    /// exists. The returned charge stays live in the original operation scope.
    fn before_value<T: Serialize + ?Sized>(
        &mut self,
        value: &T,
    ) -> Result<Self::Charge, CognitionEconomyError>;
    fn before_slots<T>(
        &mut self,
        count: usize,
        max_text_per_slot: usize,
    ) -> Result<Self::Charge, CognitionEconomyError>;
    fn digest<T: Serialize + ?Sized>(
        &mut self,
        domain: &str,
        value: &T,
    ) -> Result<String, CognitionEconomyError>;
}
pub(in crate::world) struct LegacyEconomyWorkV1;
impl CognitionEconomyWorkOpsV1 for LegacyEconomyWorkV1 {
    fn before_slots<T>(&mut self, _: usize, _: usize) -> Result<(), CognitionEconomyError> {
        Ok(())
    }
    fn work(&mut self, _: u64, _: usize) -> Result<(), CognitionEconomyError> {
        Ok(())
    }
    fn before_value<T: Serialize + ?Sized>(&mut self, _: &T) -> Result<(), CognitionEconomyError> {
        Ok(())
    }
    fn digest<T: Serialize + ?Sized>(
        &mut self,
        domain: &str,
        value: &T,
    ) -> Result<String, CognitionEconomyError> {
        Ok(cognition_digest_v1(domain, &value))
    }
}

use crate::world::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation};
use std::io::{self, Write};
mod sealed {
    pub trait Sealed {}
}
impl sealed::Sealed for LegacyEconomyWorkV1 {}
/// All returned digest copies retain their own fee until the actual derived
/// state/receipt owner drops. Large codec/clone phases are lexical guards and
/// are released when their original intermediates disappear, never accumulated.
pub(in crate::world) struct CurrentEconomyWorkFeesV1 {
    _copies: Vec<MemoryReservation>,
    _slots: MemoryReservation,
}
pub(in crate::world) struct CurrentEconomyWorkV1<'a, 'b> {
    common: std::cell::RefCell<&'a mut FormalBoundedContextV1<'b>>,
    canonical: std::cell::RefCell<&'a mut oasis7_wasm_abi::CanonicalCborWorkV1>,
    copies: Vec<MemoryReservation>,
    slots: MemoryReservation,
    first: std::cell::RefCell<Option<BodyMutationError>>,
}
impl sealed::Sealed for CurrentEconomyWorkV1<'_, '_> {}
fn stopped() -> CognitionEconomyError {
    CognitionEconomyError::InvalidState("bounded_cognition_economy_work")
}
impl<'a, 'b> CurrentEconomyWorkV1<'a, 'b> {
    pub(in crate::world) fn new(
        common: &'a mut FormalBoundedContextV1<'b>,
        canonical: &'a mut oasis7_wasm_abi::CanonicalCborWorkV1,
    ) -> Result<Self, BodyMutationError> {
        let n = 32usize
            .checked_mul(std::mem::size_of::<MemoryReservation>())
            .ok_or(BodyMutationError::Invalid("economy fee slots"))?;
        common
            .charge(1, n as u64)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let slots = common
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let copies = Vec::with_capacity(32);
        if copies.capacity() > 32 {
            return Err(BodyMutationError::Invalid("economy fee capacity"));
        }
        Ok(Self {
            common: std::cell::RefCell::new(common),
            canonical: std::cell::RefCell::new(canonical),
            copies,
            slots,
            first: std::cell::RefCell::new(None),
        })
    }
    fn fail(&mut self, error: BodyMutationError) -> CognitionEconomyError {
        if self.first.borrow().is_none() {
            *self.first.borrow_mut() = Some(error)
        }
        stopped()
    }
    fn overflow(&mut self) -> CognitionEconomyError {
        self.fail(ValueTreeError::ResourceLimited("bounded economy arithmetic").into())
    }
    fn reserve(&mut self, n: usize) -> Result<MemoryReservation, CognitionEconomyError> {
        oasis7_wasm_abi::CanonicalCborAllocator::reserve(self, n).map_err(|_| stopped())
    }
    fn keep(&mut self, fee: MemoryReservation) -> Result<(), CognitionEconomyError> {
        if self.copies.len() == self.copies.capacity() {
            let cap = self
                .copies
                .capacity()
                .checked_mul(2)
                .ok_or_else(|| self.overflow())?;
            let n = cap
                .checked_mul(std::mem::size_of::<MemoryReservation>())
                .ok_or_else(|| self.overflow())?;
            self.work(self.copies.len() as u64, n)?;
            let slots = self.reserve(n)?;
            self.copies.reserve_exact(cap - self.copies.len());
            if self.copies.capacity() > cap {
                return Err(self.fail(BodyMutationError::Invalid("economy fee capacity")));
            }
            self.slots = slots;
        }
        self.copies.push(fee);
        Ok(())
    }
    pub(in crate::world) fn finish(self) -> Result<CurrentEconomyWorkFeesV1, BodyMutationError> {
        if let Some(error) = self.first.into_inner() {
            return Err(error);
        }
        Ok(CurrentEconomyWorkFeesV1 {
            _copies: self.copies,
            _slots: self.slots,
        })
    }
}
impl CognitionEconomyWorkOpsV1 for CurrentEconomyWorkV1<'_, '_> {
    fn work(&mut self, steps: u64, bytes: usize) -> Result<(), CognitionEconomyError> {
        oasis7_wasm_abi::CanonicalCborAllocator::charge_work(self, steps, bytes as u64)
            .map_err(|_| stopped())
    }
    fn before_slots<T>(
        &mut self,
        count: usize,
        text: usize,
    ) -> Result<Self::Charge, CognitionEconomyError> {
        let node = std::mem::size_of::<T>()
            .checked_mul(11)
            .and_then(|n| {
                n.checked_add(
                    12 * std::mem::size_of::<usize>() + 128 + 8 * std::mem::align_of::<T>(),
                )
            })
            .and_then(|n| n.checked_add(text))
            .and_then(|n| n.checked_mul(2))
            .ok_or_else(|| self.overflow())?;
        let bytes = count
            .checked_mul(node)
            .and_then(|n| n.checked_add(128))
            .ok_or_else(|| self.overflow())?;
        self.work(count as u64, bytes)?;
        self.reserve(bytes)
    }
    fn before_value<T: Serialize + ?Sized>(
        &mut self,
        value: &T,
    ) -> Result<Self::Charge, CognitionEconomyError> {
        if self.first.borrow().is_some() {
            return Err(stopped());
        }
        struct Counter<'a, 'b, 'c> {
            ops: &'a mut CurrentEconomyWorkV1<'b, 'c>,
            bytes: usize,
        }
        impl Write for Counter<'_, '_, '_> {
            fn write(&mut self, b: &[u8]) -> io::Result<usize> {
                if self.ops.first.borrow().is_some() {
                    return Err(io::ErrorKind::Other.into());
                }
                self.ops
                    .work(1, b.len())
                    .map_err(|_| io::Error::from(io::ErrorKind::Other))?;
                self.bytes = match self.bytes.checked_add(b.len()) {
                    Some(n) => n,
                    None => {
                        self.ops.overflow();
                        return Err(io::Error::other("economy JSON count"));
                    }
                };
                Ok(b.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut counter = Counter {
            ops: self,
            bytes: 0,
        };
        let encoded = serde_json::to_writer(&mut counter, value);
        if counter.ops.first.borrow().is_some() {
            return Err(stopped());
        }
        if encoded.is_err() {
            return Err(counter.ops.fail(BodyMutationError::Invalid(
                "economy closed JSON serialization",
            )));
        }
        let bytes = counter
            .bytes
            .checked_mul(1024)
            .and_then(|n| n.checked_add(32768))
            .ok_or_else(|| counter.ops.overflow())?;
        // JSON has no zero-byte node: one full BTree internal node per wire
        // byte bounds nested one-entry objects as well as closed DTO clones.
        // Extra factor covers the original mutation's several immutable copies.
        counter.ops.work(counter.bytes as u64, bytes)?;
        counter.ops.reserve(bytes)
    }
    fn digest<T: Serialize + ?Sized>(
        &mut self,
        domain: &str,
        value: &T,
    ) -> Result<String, CognitionEconomyError> {
        if self.first.borrow().is_some() {
            return Err(stopped());
        }
        let result = {
            let mut common = self.common.borrow_mut();
            let mut canonical = self.canonical.borrow_mut();
            crate::world::incremental_normal_cognition_expiry::values::digest(
                domain,
                value,
                &mut **canonical,
                &mut **common,
            )
        };
        let owner = result.map_err(|e| self.fail(e))?;
        let n = owner
            .len()
            .checked_add(std::mem::size_of::<String>())
            .ok_or_else(|| self.overflow())?;
        self.work(1, owner.len())?;
        let fee = self.reserve(n)?;
        self.keep(fee)?;
        Ok((*owner).clone())
    }
}

impl oasis7_wasm_abi::CanonicalCborAllocator for LegacyEconomyWorkV1 {
    type Charge = ();
    fn reserve(&self, _: usize) -> Result<(), oasis7_wasm_abi::CanonicalCborStreamError> {
        Ok(())
    }
}
impl oasis7_wasm_abi::CapabilityCanonicalWorkV1 for LegacyEconomyWorkV1 {
    type Loan<'a> = std::cell::RefMut<'a, oasis7_wasm_abi::CanonicalCborWorkV1>;
    fn is_metered(&self) -> bool {
        false
    }
    fn loan_canonical_work(&self) -> Self::Loan<'_> {
        unreachable!("legacy has no bounded loan")
    }
}
impl oasis7_wasm_abi::CanonicalCborAllocator for CurrentEconomyWorkV1<'_, '_> {
    type Charge = MemoryReservation;
    fn charge_work(&self, s: u64, b: u64) -> Result<(), oasis7_wasm_abi::CanonicalCborStreamError> {
        if self.first.borrow().is_some() {
            return Err(oasis7_wasm_abi::CanonicalCborStreamError::Poisoned);
        }
        self.common.borrow_mut().charge(s, b).map_err(|e| {
            *self.first.borrow_mut() = Some(BodyMutationError::Original(Box::new(e)));
            oasis7_wasm_abi::CanonicalCborStreamError::ResourceLimited("economy Common")
        })
    }
    fn reserve(
        &self,
        n: usize,
    ) -> Result<MemoryReservation, oasis7_wasm_abi::CanonicalCborStreamError> {
        if self.first.borrow().is_some() {
            return Err(oasis7_wasm_abi::CanonicalCborStreamError::Poisoned);
        }
        self.common
            .borrow()
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(|e| {
                *self.first.borrow_mut() = Some(ValueTreeError::Memory(e).into());
                oasis7_wasm_abi::CanonicalCborStreamError::ResourceLimited("economy memory")
            })
    }
}
impl oasis7_wasm_abi::CapabilityCanonicalWorkV1 for CurrentEconomyWorkV1<'_, '_> {
    type Loan<'a>
        = std::cell::RefMut<'a, oasis7_wasm_abi::CanonicalCborWorkV1>
    where
        Self: 'a;
    fn record_stream_error(
        &self,
        error: oasis7_wasm_abi::CanonicalCborStreamError,
    ) -> oasis7_wasm_abi::CanonicalCborStreamError {
        if self.first.borrow().is_none() {
            *self.first.borrow_mut() = Some(BodyMutationError::Original(Box::new(error)))
        }
        oasis7_wasm_abi::CanonicalCborStreamError::Poisoned
    }
    fn is_metered(&self) -> bool {
        true
    }
    fn loan_canonical_work(&self) -> Self::Loan<'_> {
        std::cell::RefMut::map(self.canonical.borrow_mut(), |v| &mut **v)
    }
}
