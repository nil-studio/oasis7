//! Original Formal event root: BLAKE3(serde_cbor(Vec<WorldEvent>)). Only the
//! current transaction's complete, oldest-first success-note stream enters.
use super::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::MerkleNodeReader,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::incremental_world_event_decode::DecodedWorldEventV1;
use serde::{Serialize, Serializer, ser::SerializeSeq};
use std::{
    io::{self, Write},
    mem::size_of,
};
fn invalid() -> BodyMutationError {
    BodyMutationError::Invalid("normal terminal event commitment")
}
fn context(e: impl std::error::Error + Send + Sync + 'static) -> BodyMutationError {
    BodyMutationError::Original(Box::new(e))
}

/// Fixed DATA obtained only while visiting the original oldest-first notes.
#[derive(Clone, Copy)]
pub(in crate::world) struct NormalJournalIdSummaryV1 {
    first: Option<u64>,
    last: Option<u64>,
    count: u64,
    suffix_count: u64,
    wraps: u64,
}
impl NormalJournalIdSummaryV1 {
    pub(in crate::world) fn first(&self) -> Option<u64> {
        self.first
    }
    pub(in crate::world) fn last(&self) -> Option<u64> {
        self.last
    }
    pub(in crate::world) fn count(&self) -> u64 {
        self.count
    }
    pub(in crate::world) fn suffix_count(&self) -> u64 {
        self.suffix_count
    }
    pub(in crate::world) fn wraps(&self) -> u64 {
        self.wraps
    }
    fn empty() -> Self {
        Self {
            first: None,
            last: None,
            count: 0,
            suffix_count: 0,
            wraps: 0,
        }
    }
    fn push(&mut self, id: u64) -> Result<(), BodyMutationError> {
        if id == 0 {
            return Err(invalid());
        }
        if let Some(previous) = self.last {
            let (_, expected, _) = crate::World::preview_next_event_id(previous, 0);
            if id != expected {
                return Err(invalid());
            }
            if previous == u64::MAX {
                self.wraps = self.wraps.checked_add(1).ok_or_else(invalid)?;
                self.suffix_count = 0;
            }
        }
        if self.first.is_none() {
            self.first = Some(id)
        }
        self.last = Some(id);
        self.count = self.count.checked_add(1).ok_or_else(invalid)?;
        self.suffix_count = self.suffix_count.checked_add(1).ok_or_else(invalid)?;
        Ok(())
    }
}
/// DATA provenance stays attached to the actual pre-capture transaction guard.
/// Formal admission still requires the enclosing capture's exact actual seal.
pub(in crate::world) struct NormalEventCommitmentV1 {
    events: Vec<ReservedValue<DecodedWorldEventV1>>,
    _events_fee: MemoryReservation,
    journal_ids: NormalJournalIdSummaryV1,
    digest: [u8; 32],
    count: usize,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
impl NormalEventCommitmentV1 {
    /// Retain the same decoded appended originals for projection. This never
    /// reopens old journal objects or replays a producer after final hashing.
    pub(in crate::world) fn visit_appended_events_v1<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
        mut emit: impl FnMut(
            &oasis7_world_state::WorldEvent,
            &mut FormalBoundedContextV1<'_>,
        ) -> Result<(), BodyMutationError>,
    ) -> Result<(), BodyMutationError> {
        tx.validate_read_source_guard(&self.guard, live)?;
        common.consume_steps(1).map_err(context)?;
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self._fee.uses_budget(common.memory())
            || !self._events_fee.uses_budget(common.memory())
            || self.events.len() != self.count
        {
            return Err(invalid());
        }
        for event in &self.events {
            common.consume_steps(1).map_err(context)?;
            if !event.uses_budget(common.memory())
                || event.allocation_class() != MemoryClass::Ordinary
            {
                return Err(invalid());
            }
            emit(event.event(), common)?;
        }
        Ok(())
    }
    pub(in crate::world) fn journal_ids(&self) -> &NormalJournalIdSummaryV1 {
        &self.journal_ids
    }
    pub(in crate::world) fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
    pub(in crate::world) fn count(&self) -> usize {
        self.count
    }
    pub(in crate::world) fn validate_capture<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        capture_guard: &PreparedGuard,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        common.charge(1, 32).map_err(context)?;
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if !self.guard.same_cut(capture_guard)
            || self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self._fee.uses_budget(common.memory())
        {
            return Err(invalid());
        }
        Ok(())
    }
}
pub(in crate::world) struct NormalEventCommitmentBuilderV1 {
    journal_ids: NormalJournalIdSummaryV1,
    events: Vec<ReservedValue<DecodedWorldEventV1>>,
    expected: usize,
    guard: PreparedGuard,
    memory: MemoryBudget,
    core: WorkIdentityV1,
    _fee: MemoryReservation,
}
impl NormalEventCommitmentBuilderV1 {
    pub(in crate::world) fn new(
        expected: usize,
        guard: PreparedGuard,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<Self, BodyMutationError> {
        let bytes = expected
            .checked_mul(size_of::<ReservedValue<DecodedWorldEventV1>>())
            .and_then(|n| n.checked_add(size_of::<Self>()))
            .ok_or_else(invalid)?;
        common.charge(1, bytes as u64).map_err(context)?;
        let fee = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut events = Vec::new();
        events.try_reserve_exact(expected).map_err(|_| invalid())?;
        if events.capacity() > expected {
            return Err(invalid());
        }
        Ok(Self {
            journal_ids: NormalJournalIdSummaryV1::empty(),
            events,
            expected,
            guard,
            memory: common.memory().clone(),
            core: common.work().identity(),
            _fee: fee,
        })
    }
    pub(in crate::world) fn push(
        &mut self,
        event: ReservedValue<DecodedWorldEventV1>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common
            .charge(1, size_of::<ReservedValue<DecodedWorldEventV1>>() as u64)
            .map_err(context)?;
        if self.events.len() >= self.expected
            || self.core != common.work().identity()
            || !event.uses_budget(common.memory())
            || event.allocation_class() != MemoryClass::Ordinary
            || !self.memory.same_pool(common.memory())
        {
            return Err(invalid());
        }
        self.journal_ids.push(event.event().id)?;
        self.events.push(event);
        Ok(())
    }
    pub(in crate::world) fn finish(
        self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<NormalEventCommitmentV1, BodyMutationError> {
        if self.events.len() != self.expected
            || self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
        {
            return Err(invalid());
        }
        let steps = u64::try_from(self.expected)
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or_else(invalid)?;
        common
            .charge(steps, size_of::<NormalEventCommitmentV1>() as u64)
            .map_err(context)?;
        let fee = common
            .memory()
            .reserve(size_of::<NormalEventCommitmentV1>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        // Vec's Serde contract is serialize_seq(Some(len)), followed by each
        // original WorldEvent Serialize. No ABI canonical sorting is used.
        struct Events<'a>(&'a [ReservedValue<DecodedWorldEventV1>]);
        impl Serialize for Events<'_> {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                let mut seq = s.serialize_seq(Some(self.0.len()))?;
                for event in self.0 {
                    seq.serialize_element(event.event())?;
                }
                seq.end()
            }
        }
        let mut out = HashWriter {
            common,
            hash: blake3::Hasher::new(),
            first: None,
        };
        let result = serde_cbor::to_writer(&mut out, &Events(&self.events));
        if let Some(error) = out.first.take() {
            return Err(error);
        }
        result.map_err(context)?;
        out.common.charge(1, 32).map_err(context)?;
        let digest = *out.hash.finalize().as_bytes();
        // self.events and all their original owned graph reservations stay
        // alive through hash finalization, including every failure/unwind.
        Ok(NormalEventCommitmentV1 {
            events: self.events,
            _events_fee: self._fee,
            journal_ids: self.journal_ids,
            digest,
            count: self.expected,
            guard: self.guard,
            core: self.core,
            memory: self.memory,
            _fee: fee,
        })
    }
}
struct HashWriter<'a, 'c> {
    common: &'a mut FormalBoundedContextV1<'c>,
    hash: blake3::Hasher,
    first: Option<BodyMutationError>,
}
impl Write for HashWriter<'_, '_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.first.is_some() {
            return Err(io::ErrorKind::Other.into());
        }
        // Codec output and hash input each pay their real bytes before use.
        // The bound also polls cancellation inside a single large string write.
        for chunk in bytes.chunks(8192) {
            let n = u64::try_from(chunk.len())
                .ok()
                .and_then(|n| n.checked_mul(2));
            let result = match n {
                Some(n) => self.common.charge(2, n).map_err(context),
                None => Err(invalid()),
            };
            if let Err(error) = result {
                self.first = Some(error);
                return Err(io::ErrorKind::Other.into());
            }
            self.hash.update(chunk);
        }
        if bytes.is_empty() {
            if let Err(error) = self.common.charge(1, 0) {
                self.first = Some(context(error));
                return Err(io::ErrorKind::Other.into());
            }
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
