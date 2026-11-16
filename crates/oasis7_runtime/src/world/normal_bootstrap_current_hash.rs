//! One continuing canonical CBOR/SHA256 DATA adapter. Typed Common/Memory
//! first cause wins the codec marker; no encoded DOM/Vec or hash callback.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1;
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation};
use oasis7_wasm_abi::{
    CanonicalCborAllocator, CanonicalCborStreamError as Error, CanonicalCborWorkV1,
    CapabilityCanonicalWorkV1,
};
use std::cell::{RefCell, RefMut};
pub(in crate::world) struct BootstrapCanonicalMeterV1<'a, 'b, 'c, 'd> {
    pub(in crate::world) common: &'a RefCell<&'b mut FormalBoundedContextV1<'c>>,
    pub(in crate::world) canonical: &'a RefCell<&'d mut CanonicalCborWorkV1>,
    first: RefCell<Option<BodyMutationError>>,
}
impl<'a, 'b, 'c, 'd> BootstrapCanonicalMeterV1<'a, 'b, 'c, 'd> {
    pub(in crate::world) fn new(
        common: &'a RefCell<&'b mut FormalBoundedContextV1<'c>>,
        canonical: &'a RefCell<&'d mut CanonicalCborWorkV1>,
    ) -> Self {
        Self {
            common,
            canonical,
            first: RefCell::new(None),
        }
    }
    pub(in crate::world) fn check(&self) -> Result<(), BodyMutationError> {
        if let Some(e) = self.first.borrow_mut().take() {
            Err(e)
        } else {
            self.common
                .borrow_mut()
                .consume_steps(0)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))
        }
    }
}
impl CanonicalCborAllocator for BootstrapCanonicalMeterV1<'_, '_, '_, '_> {
    type Charge = MemoryReservation;
    fn charge_work(&self, s: u64, b: u64) -> Result<(), Error> {
        if self.first.borrow().is_some() {
            return Err(Error::Poisoned);
        }
        self.common.borrow_mut().charge(s, b).map_err(|e| {
            *self.first.borrow_mut() = Some(BodyMutationError::Original(Box::new(e)));
            Error::ResourceLimited("Bootstrap canonical Common")
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
                Error::ResourceLimited("Bootstrap canonical memory")
            })
    }
}
impl CapabilityCanonicalWorkV1 for BootstrapCanonicalMeterV1<'_, '_, '_, '_> {
    type Loan<'a>
        = RefMut<'a, CanonicalCborWorkV1>
    where
        Self: 'a;
    fn record_stream_error(&self, error: Error) -> Error {
        if self.first.borrow().is_none() {
            *self.first.borrow_mut() = Some(BodyMutationError::Original(Box::new(error)))
        }
        Error::Poisoned
    }
    fn is_metered(&self) -> bool {
        true
    }
    fn loan_canonical_work(&self) -> Self::Loan<'_> {
        RefMut::map(self.canonical.borrow_mut(), |v| &mut **v)
    }
}
pub(in crate::world) fn canonical_hash_v1<T: serde::Serialize + ?Sized>(
    value: &T,
    common: &mut FormalBoundedContextV1<'_>,
    canonical: &mut CanonicalCborWorkV1,
) -> Result<ReservedValue<String>, BodyMutationError> {
    let common = RefCell::new(common);
    let canonical = RefCell::new(canonical);
    let meter = BootstrapCanonicalMeterV1::new(&common, &canonical);
    let output = oasis7_wasm_abi::canonical_hash_with_work_v1(value, &meter);
    meter.check()?;
    let output = output.map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let charged = meter.charge_work(1, 64);
    meter.check()?;
    charged.map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let fee = meter.reserve(std::mem::size_of::<String>() + 64);
    meter.check()?;
    let fee = fee.map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    Ok(fee.retain(output.as_str().to_owned()))
}

pub(in crate::world) fn with_current_meter_v1<T>(
    common: &mut FormalBoundedContextV1<'_>,
    canonical: &mut CanonicalCborWorkV1,
    run: impl FnOnce(&BootstrapCanonicalMeterV1<'_, '_, '_, '_>) -> T,
) -> Result<T, BodyMutationError> {
    let common = RefCell::new(common);
    let canonical = RefCell::new(canonical);
    let meter = BootstrapCanonicalMeterV1::new(&common, &canonical);
    let output = run(&meter);
    meter.check()?;
    Ok(output)
}
pub(in crate::world) fn grant_hash_matches_v1(
    grant: &oasis7_wasm_abi::CapabilityGrantV2,
    expected: &str,
    common: &mut FormalBoundedContextV1<'_>,
    canonical: &mut CanonicalCborWorkV1,
) -> Result<bool, BodyMutationError> {
    with_current_meter_v1(common, canonical, |meter| {
        oasis7_wasm_abi::canonical_hash_with_work_v1(&grant.canonical_body_input_v1(), meter)
            .map(|v| v.as_str() == expected)
    })?
    .map_err(|e| BodyMutationError::Original(Box::new(e)))
}

pub(in crate::world) fn validate_registered_grant_v1(
    grant: &oasis7_wasm_abi::CapabilityGrantV2,
    tick: u64,
    common: &mut FormalBoundedContextV1<'_>,
    canonical: &mut CanonicalCborWorkV1,
) -> Result<Result<(), crate::WorldError>, BodyMutationError> {
    with_current_meter_v1(common, canonical, |meter| {
        crate::world::capability_authorization::validate_registered_grant_input_with_work_v1(
            grant,
            tick,
            meter,
            &mut |s, b| meter.charge_work(s, b),
        )
    })?
    .map_err(|e| BodyMutationError::Original(Box::new(e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis7_formal_protocol::controlled_authority::formal::{
        FormalError, bounded_context::FormalBoundedWorkV1,
    };
    fn quota() -> CanonicalCborWorkV1 {
        CanonicalCborWorkV1 {
            visits_remaining: 10000,
            bytes_remaining: 1 << 20,
            max_depth: 32,
            max_map_keys: 16,
            max_key_bytes: 4096,
        }
    }
    #[test]
    fn canonical_common_failure_stays_typed_and_never_becomes_business() {
        let memory = oasis7_proto::memory_budget::MemoryBudget::new(1 << 20, 0).unwrap();
        for (steps, cancel, label) in [
            (0, false, "bounded archive read steps"),
            (100, true, "bounded archive read cancelled"),
        ] {
            let work = FormalBoundedWorkV1::new(steps, 1 << 20).unwrap();
            let mut calls = 0;
            let mut stop = || {
                calls += 1;
                cancel
            };
            let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
            let error =
                match canonical_hash_v1(&serde_json::json!({"a":"b"}), &mut common, &mut quota()) {
                    Err(e) => e,
                    Ok(_) => panic!("missing actual Common refusal"),
                };
            let BodyMutationError::Original(error) = error else {
                panic!("lost typed Common cause")
            };
            assert!(
                matches!(error.downcast_ref::<FormalError>(),Some(FormalError::Invalid(s))if s==label)
            );
            drop(common);
            assert_eq!(calls, 1);
        }
    }
    #[test]
    fn canonical_codec_allowance_stays_infrastructure() {
        let memory = oasis7_proto::memory_budget::MemoryBudget::new(1 << 20, 0).unwrap();
        let work = FormalBoundedWorkV1::new(10000, 1 << 20).unwrap();
        let mut stop = || false;
        let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        let mut codec = quota();
        codec.max_map_keys = 0;
        let error = match canonical_hash_v1(&serde_json::json!({"a":"b"}), &mut common, &mut codec)
        {
            Err(e) => e,
            Ok(_) => panic!("missing actual codec allowance refusal"),
        };
        let BodyMutationError::Original(error) = error else {
            panic!("lost original codec cause")
        };
        assert!(matches!(
            error.downcast_ref::<Error>(),
            Some(Error::ResourceLimited(_))
        ));
    }
}
