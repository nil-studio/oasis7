//! Object-safe archive data helpers. The opaque owner records a genuine local
//! Ordinary producer, not a caller assertion about an unobservable class.
use super::archive_transfer::FormalArchiveSourceV1;
use super::{FormalError, invalid};
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::{
    ChildLeaseV1, FormalBoundedContextV1, FormalBoundedWorkV1, WorkIdentityV1,
};
use oasis7_proto::memory_budget::{MemoryBudget, ReservedValue};
use std::{path::Path, sync::Arc};
/// Compatibility facade over exactly one common meter; no mirrored balances.
pub struct FormalArchiveReadWorkV1 {
    core: FormalBoundedWorkV1,
}
impl FormalArchiveReadWorkV1 {
    pub fn new(steps: u64, read_bytes: u64) -> Result<Self, FormalError> {
        Ok(Self {
            core: FormalBoundedWorkV1::new(steps, read_bytes)?,
        })
    }
    pub fn remaining_steps(&self) -> u64 {
        self.core.remaining_steps()
    }
    pub fn remaining_read_bytes(&self) -> u64 {
        self.core.remaining_read_bytes()
    }
    pub fn charge(
        &self,
        steps: u64,
        read_bytes: u64,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<(), FormalError> {
        self.core.charge(steps, read_bytes, cancelled)
    }
    pub fn consume_steps(
        &self,
        n: u64,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<(), FormalError> {
        self.core.consume_steps(n, cancelled)
    }
    pub fn consume_read_bytes(
        &self,
        n: u64,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<(), FormalError> {
        self.core.consume_read_bytes(n, cancelled)
    }
    pub fn bounded_context<'a>(
        &'a self,
        memory: &'a MemoryBudget,
        cancelled: &'a mut dyn FnMut() -> bool,
    ) -> FormalBoundedContextV1<'a> {
        FormalBoundedContextV1::new(memory, &self.core, cancelled)
    }
    fn is_child_active(&self) -> bool {
        self.core.is_child_active()
    }
    fn step(&self, cancelled: &mut dyn FnMut() -> bool) -> Result<(), FormalError> {
        self.charge(1, 0, cancelled)
    }
    fn bytes(&self, n: usize, cancelled: &mut dyn FnMut() -> bool) -> Result<(), FormalError> {
        self.consume_read_bytes(
            u64::try_from(n).map_err(|_| invalid("bounded archive byte layout"))?,
            cancelled,
        )
    }
    pub(in crate::controlled_authority::formal::endpoint) fn with_child_counters<T>(
        &self,
        cancelled: &mut dyn FnMut() -> bool,
        run: impl FnOnce(u64, u64, &mut dyn FnMut() -> bool) -> (Result<T, FormalError>, u64, u64),
    ) -> Result<T, FormalError> {
        self.step(cancelled)?;
        let lease = self.core.begin_child()?;
        let before = (lease.remaining_steps(), lease.remaining_read_bytes());
        let (result, steps, bytes) = run(before.0, before.1, cancelled);
        Self::reconcile_child(lease, before, (steps, bytes), result)
    }
    // Only this trusted adapter sees legacy scalar snapshots. Common providers
    // receive no arbitrary after-counter or refund operation.
    fn reconcile_child<T>(
        lease: ChildLeaseV1<'_>,
        before: (u64, u64),
        after: (u64, u64),
        result: Result<T, FormalError>,
    ) -> Result<T, FormalError> {
        let debit = match (before.0.checked_sub(after.0), before.1.checked_sub(after.1)) {
            (Some(steps), Some(bytes)) => {
                let debit = lease.consume(steps, bytes);
                lease.finish();
                debit
            }
            _ => {
                // Abandoning the exclusive lease fences both balances.
                drop(lease);
                Err(invalid("bounded archive child counters increased"))
            }
        };
        match result {
            Err(error) => Err(error),
            Ok(value) => {
                debit?;
                Ok(value)
            }
        }
    }
    #[cfg(test)]
    fn reconcile<T>(
        &self,
        before: (u64, u64),
        after: (u64, u64),
        result: Result<T, FormalError>,
    ) -> Result<T, FormalError> {
        Self::reconcile_child(self.core.begin_child()?, before, after, result)
    }
}

/// No public constructor, conversion, Clone or detachable Vec. Safe external
/// providers may forward a genuine owner obtained from a File archive source.
/// Only this module's local Ordinary-loader path can mint an owner.
#[derive(Debug)]
pub struct FormalArchiveReadOwnerV1 {
    bytes: ReservedValue<Vec<u8>>,
    meter_id: WorkIdentityV1,
}
impl FormalArchiveReadOwnerV1 {
    pub fn as_slice(&self) -> &[u8] {
        self.bytes.as_slice()
    }
    pub fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.bytes.uses_budget(memory)
    }
    pub fn reserved_bytes(&self) -> usize {
        self.bytes.reserved_bytes()
    }
    fn validate(
        &self,
        id: &str,
        max: usize,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<(), FormalError> {
        work.step(cancelled)?;
        let required = self
            .bytes
            .capacity()
            .checked_add(std::mem::size_of::<Vec<u8>>())
            .ok_or_else(|| invalid("bounded archive owner layout"))?;
        if self.meter_id != work.core.identity() {
            return Err(invalid("bounded archive owner meter mismatch"));
        }
        if !self.uses_budget(memory) || self.reserved_bytes() < required || self.bytes.len() > max {
            return Err(invalid("bounded archive owner pool/charge/cap"));
        }
        let mut hash = blake3::Hasher::new();
        for chunk in self.as_slice().chunks(65536) {
            work.bytes(chunk.len(), cancelled)?;
            hash.update(chunk);
        }
        if hash.finalize().to_hex().as_str() != id {
            return Err(invalid("bounded archive owner checksum"));
        }
        work.step(cancelled)
    }
}
pub(super) fn read_local_archive_bounded_v1(
    directory: &Path,
    id: &str,
    max: usize,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<Option<FormalArchiveReadOwnerV1>, FormalError> {
    work.step(cancelled)?;
    super::super::crypto::hash_hex(id)?;
    let capacity = directory
        .as_os_str()
        .as_encoded_bytes()
        .len()
        .checked_add(65)
        .ok_or_else(|| invalid("bounded archive path layout"))?;
    let path_bytes = capacity
        .checked_add(std::mem::size_of::<std::path::PathBuf>())
        .ok_or_else(|| invalid("bounded archive path layout"))?;
    let _path_fee = memory
        .reserve(
            path_bytes,
            oasis7_proto::memory_budget::MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("bounded archive path admission"))?;
    let mut path = std::path::PathBuf::with_capacity(capacity);
    if path.capacity() > capacity {
        return Err(invalid("bounded archive path capacity"));
    }
    path.push(directory);
    path.push(id);
    let lease = work.core.begin_child()?;
    let before = (work.remaining_steps(), work.remaining_read_bytes());
    let mut selected = super::super::physical::GenerationSelectedReadWorkV1 {
        remaining_steps: before.0,
        remaining_read_bytes: before.1,
    };
    let mut stop = || cancelled();
    let result = super::super::physical::with_selected_path_bounded_v1(
        &path,
        id,
        memory,
        &mut selected,
        &mut stop,
        |path, w, stop| {
            #[cfg(unix)]
            {
                let mut physical = super::super::physical::PhysicalReadWorkV1 {
                    remaining_steps: w.remaining_steps,
                    remaining_read_bytes: w.remaining_read_bytes,
                    remaining_metadata_bytes: w.remaining_read_bytes,
                };
                let result = super::super::physical::read_unknown_length_durable_v1(
                    path,
                    id,
                    max,
                    memory,
                    &mut physical,
                    stop,
                );
                w.remaining_steps = physical.remaining_steps;
                w.remaining_read_bytes = physical.remaining_read_bytes;
                if let Some(bytes) = result? {
                    return Ok(Some(bytes));
                }
            }
            #[cfg(not(unix))]
            if super::super::physical::enabled(
                path.parent()
                    .ok_or_else(|| invalid("bounded archive selected parent"))?,
            )? {
                return Err(invalid("bounded archive physical reader requires Unix"));
            }
            let mut legacy = super::super::business_pack::LegacyReadWorkV1 {
                remaining_steps: w.remaining_steps,
                remaining_read_bytes: w.remaining_read_bytes,
            };
            let result = super::super::business_pack::read_legacy_unknown_length_durable_v1(
                path,
                id,
                max,
                memory,
                &mut legacy,
                stop,
            );
            w.remaining_steps = legacy.remaining_steps;
            w.remaining_read_bytes = legacy.remaining_read_bytes;
            result
        },
    );
    let result = FormalArchiveReadWorkV1::reconcile_child(
        lease,
        before,
        (selected.remaining_steps, selected.remaining_read_bytes),
        result,
    );
    let owner = result?.map(|bytes| FormalArchiveReadOwnerV1 {
        bytes,
        meter_id: work.core.identity(),
    });
    if let Some(owner) = &owner {
        owner.validate(id, max, memory, work, cancelled)?;
    }
    work.step(cancelled)?;
    Ok(owner)
}
/// First error is terminal, including explicit unsupported. Only genuine None
/// may advance to another source, with precisely the same remaining counters.
pub(super) fn read_archive_chain_bounded_v1(
    local: &dyn FormalArchiveSourceV1,
    fallback: &[Arc<dyn FormalArchiveSourceV1>],
    id: &str,
    max: usize,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<Option<FormalArchiveReadOwnerV1>, FormalError> {
    super::super::crypto::hash_hex(id)?;
    work.step(cancelled)?;
    if let Some(owner) = local.read_object_bounded(id, max, memory, work, cancelled)? {
        owner.validate(id, max, memory, work, cancelled)?;
        return Ok(Some(owner));
    }
    read_archive_sources_bounded_v1(fallback, id, max, memory, work, cancelled)
}
/// Shared source suffix for Store and LocalArchive. No File constructor or
/// whole-object compatibility method is invoked to enter this path.
pub(super) fn read_archive_sources_bounded_v1(
    sources: &[Arc<dyn FormalArchiveSourceV1>],
    id: &str,
    max: usize,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<Option<FormalArchiveReadOwnerV1>, FormalError> {
    super::super::crypto::hash_hex(id)?;
    for source in sources {
        work.step(cancelled)?;
        if let Some(owner) = source.read_object_bounded(id, max, memory, work, cancelled)? {
            owner.validate(id, max, memory, work, cancelled)?;
            return Ok(Some(owner));
        }
    }
    work.step(cancelled)?;
    Ok(None)
}

#[cfg(all(test, unix))]
#[path = "endpoint_archive_bounded_read_tests.rs"]
mod tests;
