//! Fixed self-contained Frozen original source. Content is not acceptance.
#[cfg(all(test, unix))]
#[path = "endpoint_incremental_normal_frozen_material_tests.rs"]
mod tests;

use super::*;
use crate::controlled_authority::formal::endpoint::store::bounded_request::PaidRequestStateV1;
use oasis7_formal_protocol::incremental_material::{
    OriginalObjectReader, OriginalObjectStreamReader,
};
use std::{
    cell::RefCell,
    io::{self, Write},
};

struct FrozenOriginals<'a, 's> {
    paid: &'a PaidRequestStateV1,
    memory: &'a MemoryBudget,
    work: &'a FormalArchiveReadWorkV1,
    stop: &'a RefCell<&'s mut dyn FnMut() -> bool>,
}
impl FrozenOriginals<'_, '_> {
    fn original(&self, id: &[u8; 32], memory: &MemoryBudget) -> Result<&[u8], FormalError> {
        if !self.paid.uses_budget(memory) || !memory.same_pool(self.memory) {
            return Err(invalid("normal Frozen original pool"));
        }
        let FormalRequestStateV1::Frozen { material, .. } = self.paid.state() else {
            return Err(invalid("normal Frozen original source variant"));
        };
        self.work.consume_steps(1, &mut **self.stop.borrow_mut())?;
        for object in &material.objects {
            self.work.charge(
                1,
                object.content_hash.len() as u64,
                &mut **self.stop.borrow_mut(),
            )?;
            let mut hash = [0; 32];
            if hex::decode_to_slice(&object.content_hash, &mut hash).is_ok() && &hash == id {
                return Ok(object.bytes.as_slice());
            }
        }
        Err(invalid("normal Frozen original missing"))
    }
}
impl OriginalObjectReader for FrozenOriginals<'_, '_> {
    type Error = FormalError;
    fn allocation_budget(&self) -> &MemoryBudget {
        self.memory
    }
    fn allocation_class(&self) -> MemoryClass {
        MemoryClass::Ordinary
    }
    fn read_original(
        &mut self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
    ) -> Result<std::sync::Arc<ReservedValue<Vec<u8>>>, FormalError> {
        // The shared verifier loads bounded config/manifest/native metadata;
        // input, policy and continuation remain on the streaming path below.
        let bytes = self.original(id, memory)?;
        if bytes.len() > max {
            return Err(invalid("normal Frozen original bound"));
        }
        let retained_bytes = bytes
            .len()
            .checked_add(size_of::<ReservedValue<Vec<u8>>>() + 2 * size_of::<usize>())
            .ok_or_else(|| invalid("normal Frozen original layout"))?;
        let fee = memory
            .reserve(retained_bytes, MemoryClass::Ordinary)
            .map_err(|_| invalid("normal Frozen original admission"))?;
        let mut retained = Vec::new();
        retained
            .try_reserve_exact(bytes.len())
            .map_err(|_| invalid("normal Frozen original allocation"))?;
        // The Arc shell allowance is not spare capacity for the payload.
        if retained.capacity() > bytes.len() {
            return Err(invalid("normal Frozen original capacity"));
        }
        for chunk in bytes.chunks(8192) {
            self.work
                .charge(1, chunk.len() as u64, &mut **self.stop.borrow_mut())?;
            retained.extend_from_slice(chunk);
        }
        self.work.consume_steps(1, &mut **self.stop.borrow_mut())?;
        Ok(std::sync::Arc::new(fee.retain(retained)))
    }
}
impl OriginalObjectStreamReader for FrozenOriginals<'_, '_> {
    fn stream_original(
        &mut self,
        id: &[u8; 32],
        expected: u64,
        memory: &MemoryBudget,
        output: &mut dyn Write,
    ) -> Result<u64, Self::Error> {
        let bytes = self.original(id, memory)?;
        if bytes.len() as u64 != expected {
            return Err(invalid("normal Frozen original length"));
        }
        for chunk in bytes.chunks(8192) {
            let mut offset = 0;
            while offset < chunk.len() {
                self.work.charge(
                    1,
                    (chunk.len() - offset) as u64,
                    &mut **self.stop.borrow_mut(),
                )?;
                match output.write(&chunk[offset..]) {
                    Ok(0) => return Err(invalid("normal Frozen original zero write")),
                    Ok(n) if n <= chunk.len() - offset => offset += n,
                    Ok(_) => return Err(invalid("normal Frozen original invalid write length")),
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(invalid(format!("normal Frozen original sink: {e}"))),
                }
            }
        }
        self.work.consume_steps(1, &mut **self.stop.borrow_mut())?;
        Ok(expected)
    }
}
pub(super) fn check_material<'a>(
    paid: &'a PaidRequestStateV1,
    reservation: &'a VerifiedFormalReservationV1,
    parent: &'a VerifiedFormalCommitV1,
    memory: &MemoryBudget,
    common: &FormalArchiveReadWorkV1,
    legacy: &mut IncrementalFrozenMaterialWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<ReservedValue<CheckedIncrementalFrozenMaterialV1<'a>>, FormalError> {
    let FormalRequestStateV1::Frozen { material, .. } = paid.state() else {
        return Err(invalid("normal Frozen material source variant"));
    };
    let bytes = legacy
        .comparison_bytes_remaining
        .checked_add(legacy.hash_bytes_remaining)
        .and_then(|n| n.checked_add(legacy.frozen_wire_bytes_remaining))
        .and_then(|n| n.checked_add(legacy.original_bytes_remaining))
        .ok_or_else(|| invalid("normal Frozen work layout"))?;
    let steps = bytes
        .checked_add(legacy.metadata_entries_remaining)
        .and_then(|n| n.checked_add(legacy.original_calls_remaining))
        .and_then(|n| n.checked_add(4096))
        .ok_or_else(|| invalid("normal Frozen work layout"))?;
    // Conservative full legacy upper bound is consumed once before its fixed
    // kernel. It is never refunded or installed as a new Common account.
    common.charge(steps, bytes, stop)?;
    let stop = RefCell::new(stop);
    let mut source = FrozenOriginals {
        paid,
        memory,
        work: common,
        stop: &stop,
    };
    verify_incremental_frozen_material_v1(
        reservation,
        parent,
        material,
        &mut source,
        memory,
        legacy,
        || (**stop.borrow_mut())(),
    )
    .map_err(|error| match error {
        IncrementalFrozenMaterialErrorV1::Source(cause)
        | IncrementalFrozenMaterialErrorV1::Formal(cause) => cause,
        IncrementalFrozenMaterialErrorV1::Cancelled => invalid("bounded archive read cancelled"),
        other => invalid(format!("normal Frozen material: {other}")),
    })
}
