//! Fixed original canonical encoder with the actual continuing caller loan.
use super::*;
use oasis7_wasm_abi::{
    CanonicalCborAllocator, CanonicalCborStreamError as Error, CanonicalCborWorkV1,
    CapabilityCanonicalWorkV1,
};
use std::cell::{RefCell, RefMut};
struct Meter<'a, 'b, 'c, 'd> {
    state: &'a RefCell<BuildState<'b, 'c>>,
    canonical: &'a RefCell<&'d mut CanonicalCborWorkV1>,
}
impl CanonicalCborAllocator for Meter<'_, '_, '_, '_> {
    type Charge = MemoryReservation;
    fn charge_work(&self, steps: u64, bytes: u64) -> Result<(), Error> {
        let mut s = self.state.borrow_mut();
        if s.first.is_some() {
            return Err(Error::Poisoned);
        }
        s.common.charge(steps, bytes).map_err(|e| {
            s.first = Some(fail(e, None));
            Error::Poisoned
        })
    }
    fn reserve(&self, bytes: usize) -> Result<MemoryReservation, Error> {
        let mut s = self.state.borrow_mut();
        if s.first.is_some() {
            return Err(Error::Poisoned);
        }
        s.common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(|e| {
                s.first = Some(fail(invalid(e.to_string()), None));
                Error::Poisoned
            })
    }
}
impl CapabilityCanonicalWorkV1 for Meter<'_, '_, '_, '_> {
    type Loan<'a>
        = RefMut<'a, CanonicalCborWorkV1>
    where
        Self: 'a;
    fn is_metered(&self) -> bool {
        true
    }
    fn loan_canonical_work(&self) -> Self::Loan<'_> {
        RefMut::map(self.canonical.borrow_mut(), |v| &mut **v)
    }
    fn record_stream_error(&self, error: Error) -> Error {
        let mut s = self.state.borrow_mut();
        if s.first.is_none() {
            s.first = Some(PaidFormalBusinessErrorV1 {
                error: invalid("business canonical codec"),
                protocol: None,
                body: Some(
                    oasis7_world_state::incremental_agent_mutation::BodyMutationError::Original(
                        Box::new(error),
                    ),
                ),
                _phase: None,
            })
        }
        Error::Poisoned
    }
}
struct Sink<'a, 'b, 'c, 'd, 'e> {
    meter: &'a Meter<'b, 'c, 'd, 'e>,
    bytes: usize,
    output: Option<Vec<u8>>,
}
impl Write for Sink<'_, '_, '_, '_, '_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.meter
            .charge_work(1, bytes.len() as u64)
            .map_err(|_| io::ErrorKind::Other)?;
        let next = self
            .bytes
            .checked_add(bytes.len())
            .ok_or(io::ErrorKind::Other)?;
        if let Some(out) = &mut self.output {
            if next > out.capacity() {
                return Err(io::ErrorKind::Other.into());
            }
            out.extend_from_slice(bytes)
        }
        self.bytes = next;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub(super) fn encode<T: Serialize>(
    value: &T,
    state: &RefCell<BuildState<'_, '_>>,
    canonical: &RefCell<&mut CanonicalCborWorkV1>,
) -> Result<Vec<u8>, String> {
    let meter = Meter { state, canonical };
    let mut count = Sink {
        meter: &meter,
        bytes: 0,
        output: None,
    };
    if oasis7_wasm_abi::stream_canonical_cbor_with_work_v1(value, &meter, &mut count).is_err() {
        return Err(INDEX_FAILED.into());
    }
    let mut fee = meter
        .reserve(count.bytes)
        .map_err(|_| INDEX_FAILED.to_owned())?;
    let mut output = Sink {
        meter: &meter,
        bytes: 0,
        output: Some(Vec::with_capacity(count.bytes)),
    };
    let result = oasis7_wasm_abi::stream_canonical_cbor_with_work_v1(value, &meter, &mut output);
    // Retain all canonical outputs through the builder; the enclosing phase owns
    // the same bytes after the original kernel moves them into the index update.
    let mut s = state.borrow_mut();
    match &mut s.leaf_phase {
        Some(held) => held.merge(&mut fee).map_err(|e| e.to_string())?,
        None => s.leaf_phase = Some(fee),
    }
    if result.is_err() {
        return Err(INDEX_FAILED.into());
    }
    if output.bytes != count.bytes {
        return Err("business canonical size changed".into());
    }
    Ok(output.output.take().expect("fixed output sink"))
}
