//! One canonical producer, three actual streaming object stores. A failure
//! propagates before the Host can publish any active root or Formal candidate.
use oasis7_distfs::incremental_page_store::{
    PageIoBudget, PageStoreError,
    objects::{IncrementalObjectStore, ObjectIo, ObjectStreamWriteError},
};
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        FormalError,
        bounded_context::{ChildLeaseV1, FormalBoundedChargeErrorV1, FormalBoundedContextV1},
    },
    incremental_artifacts::OriginalArtifactRefV1,
    incremental_value::{ValueIoBudget, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation};
use oasis7_runtime::{
    BodyMutationError,
    incremental_action_original::{
        ActionOriginalReceiptV1, ActionOriginalStreamV1, persist_world_event_original_with_work_v1,
    },
};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use std::{
    cell::{Cell, RefCell},
    io::{self, Write},
    sync::atomic::AtomicBool,
};
pub(super) struct Originals<'a, 'store, 'work> {
    pub(super) stores: &'a [IncrementalObjectStore<'store>; 3],
    pub(super) physical: &'work mut [PageIoBudget; 3],
    pub(super) logical: &'work mut [ValueIoBudget; 3],
    pub(super) occurrences: &'work mut u64,
    pub(super) max_bytes: usize,
    pub(super) cancelled: &'a AtomicBool,
    pub(super) error: &'a RefCell<Option<PageStoreError>>,
}
fn store_error(error: PageStoreError, slot: &RefCell<Option<PageStoreError>>) -> BodyMutationError {
    if slot.borrow().is_none() {
        *slot.borrow_mut() = Some(error);
    }
    BodyMutationError::Invalid("IWS original replica storage failed")
}
fn stream_error(
    error: ObjectStreamWriteError<BodyMutationError>,
    slot: &RefCell<Option<PageStoreError>>,
) -> BodyMutationError {
    match error {
        ObjectStreamWriteError::Producer(e) => e,
        ObjectStreamWriteError::Store(e) => store_error(e, slot),
    }
}
struct Tee<'a> {
    writers: [&'a mut dyn Write; 3],
    failed: bool,
}
impl Write for Tee<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.failed {
            return Err(io::Error::other("IWS original fanout poisoned"));
        }
        self.failed = true;
        for writer in &mut self.writers {
            writer.write_all(bytes)?;
        }
        self.failed = false;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.failed {
            return Err(io::Error::other("IWS original fanout poisoned"));
        }
        self.failed = true;
        for writer in &mut self.writers {
            writer.flush()?;
        }
        self.failed = false;
        Ok(())
    }
}

fn context_error(error: FormalError) -> BodyMutationError {
    BodyMutationError::Original(Box::new(error))
}

/// A finite share of the ORIGINAL child, not another Core or caller account.
/// The codec calls this before every actual work request, including its map
/// comparison allowance, which is absent from canonical.bytes_remaining.
struct CodecShare<'a> {
    initial: [u64; 2],
    remaining: Cell<[u64; 2]>,
    stop: RefCell<&'a mut dyn FnMut() -> bool>,
    cancelled: Cell<bool>,
}
impl CodecShare<'_> {
    fn cancelled(&self) -> bool {
        if self.cancelled.get() || (self.stop.borrow_mut())() {
            self.cancelled.set(true);
            true
        } else {
            false
        }
    }
    fn charge(&self, steps: u64, bytes: u64) -> Result<(), BodyMutationError> {
        if self.cancelled() {
            return Err(context_error(FormalBoundedChargeErrorV1::Cancelled.into()));
        }
        let before = self.remaining.get();
        self.remaining.set([
            before[0].saturating_sub(steps),
            before[1].saturating_sub(bytes),
        ]);
        if before[0] < steps {
            return Err(context_error(FormalBoundedChargeErrorV1::StepLimit.into()));
        }
        if before[1] < bytes {
            return Err(context_error(FormalBoundedChargeErrorV1::ByteLimit.into()));
        }
        Ok(())
    }
}

struct EventMeterFrame<'a> {
    before: [[u64; 7]; 3],
    capped: [[u64; 7]; 3],
    after: [[u64; 7]; 3],
    used: [u64; 2],
    codec: CodecShare<'a>,
    lease: ChildLeaseV1<'a>,
}

/// Only the concrete borrowed DTOs below can enter this private child.
/// Static dispatch retains one source reference and adds no owned frame.
trait BoundedOriginalSource {
    type Work;
    type Receipt;
    const POOL_ERROR: &'static str;
    const LANE_ERROR: &'static str;
    fn persist<C: FnMut() -> bool>(
        &self,
        memory: &MemoryBudget,
        canonical: &mut Self::Work,
        cancelled: &mut C,
        max_bytes: usize,
        originals: &mut Originals<'_, '_, '_>,
        debit: &dyn Fn(u64, u64) -> Result<(), BodyMutationError>,
    ) -> Result<Self::Receipt, BodyMutationError>;
    fn reference(receipt: Self::Receipt) -> OriginalArtifactRefV1;
}
impl BoundedOriginalSource for oasis7_world_state::WorldEvent {
    type Work = CanonicalCborWorkV1;
    type Receipt = ActionOriginalReceiptV1;
    const POOL_ERROR: &'static str = "GEN journal original source pool";
    const LANE_ERROR: &'static str = "GEN journal original lane increased";
    fn persist<C: FnMut() -> bool>(
        &self,
        memory: &MemoryBudget,
        canonical: &mut CanonicalCborWorkV1,
        cancelled: &mut C,
        max_bytes: usize,
        originals: &mut Originals<'_, '_, '_>,
        debit: &dyn Fn(u64, u64) -> Result<(), BodyMutationError>,
    ) -> Result<Self::Receipt, BodyMutationError> {
        persist_world_event_original_with_work_v1(
            self,
            memory,
            canonical,
            cancelled,
            max_bytes,
            &mut |source: &mut dyn ActionOriginalStreamV1, poll: &mut dyn FnMut() -> bool| {
                originals.stream(source, poll)
            },
            debit,
        )
    }
    fn reference(receipt: Self::Receipt) -> OriginalArtifactRefV1 {
        OriginalArtifactRefV1 {
            id: receipt.original_id(),
            byte_len: receipt.original_bytes(),
        }
    }
}
impl BoundedOriginalSource for oasis7_world_state::snapshot::SnapshotRecord {
    type Work = CanonicalCborWorkV1;
    type Receipt = OriginalArtifactRefV1;
    const POOL_ERROR: &'static str = "GEN snapshot original source pool";
    const LANE_ERROR: &'static str = "GEN snapshot original lane increased";
    fn persist<C: FnMut() -> bool>(
        &self,
        memory: &MemoryBudget,
        canonical: &mut CanonicalCborWorkV1,
        cancelled: &mut C,
        max_bytes: usize,
        originals: &mut Originals<'_, '_, '_>,
        debit: &dyn Fn(u64, u64) -> Result<(), BodyMutationError>,
    ) -> Result<Self::Receipt, BodyMutationError> {
        oasis7_world_state::incremental_runtime_history_original::persist_snapshot_record_original_with_work_v1(
            self, memory, canonical, cancelled, max_bytes,
            &mut |source: &mut dyn ActionOriginalStreamV1, poll: &mut dyn FnMut() -> bool| {
                originals.stream(source, poll)
            }, debit,
        )
    }
    fn reference(receipt: Self::Receipt) -> OriginalArtifactRefV1 {
        receipt
    }
}
impl BoundedOriginalSource for oasis7_world_state::consensus::TickConsensusRecord {
    type Work = CanonicalCborWorkV1;
    type Receipt = OriginalArtifactRefV1;
    const POOL_ERROR: &'static str = "GEN hot Tick original source pool";
    const LANE_ERROR: &'static str = "GEN hot Tick original lane increased";
    fn persist<C: FnMut() -> bool>(
        &self,
        memory: &MemoryBudget,
        canonical: &mut CanonicalCborWorkV1,
        cancelled: &mut C,
        max_bytes: usize,
        originals: &mut Originals<'_, '_, '_>,
        debit: &dyn Fn(u64, u64) -> Result<(), BodyMutationError>,
    ) -> Result<Self::Receipt, BodyMutationError> {
        oasis7_world_state::incremental_runtime_history_original::persist_tick_consensus_record_original_with_work_v1(
            self, memory, canonical, cancelled, max_bytes,
            &mut |source: &mut dyn ActionOriginalStreamV1, poll: &mut dyn FnMut() -> bool| {
                originals.stream(source, poll)
            }, debit,
        )
    }
    fn reference(receipt: Self::Receipt) -> OriginalArtifactRefV1 {
        receipt
    }
}
impl BoundedOriginalSource for oasis7_world_state::consensus::TickConsensusRejectionAuditEvent {
    type Work = CanonicalCborWorkV1;
    type Receipt = OriginalArtifactRefV1;
    const POOL_ERROR: &'static str = "GEN retained audit original source pool";
    const LANE_ERROR: &'static str = "GEN retained audit original lane increased";
    fn persist<C: FnMut() -> bool>(
        &self,
        memory: &MemoryBudget,
        canonical: &mut CanonicalCborWorkV1,
        cancelled: &mut C,
        max_bytes: usize,
        originals: &mut Originals<'_, '_, '_>,
        debit: &dyn Fn(u64, u64) -> Result<(), BodyMutationError>,
    ) -> Result<Self::Receipt, BodyMutationError> {
        oasis7_world_state::incremental_runtime_history_original::persist_tick_consensus_rejection_audit_original_with_work_v1(
            self, memory, canonical, cancelled, max_bytes,
            &mut |source: &mut dyn ActionOriginalStreamV1, poll: &mut dyn FnMut() -> bool| {
                originals.stream(source, poll)
            }, debit,
        )
    }
    fn reference(receipt: Self::Receipt) -> OriginalArtifactRefV1 {
        receipt
    }
}

/// The preceding borrowed producer already paid for the exact raw digest.
/// This immediate adapter neither measures nor retains another source.
struct RawOriginalSource<'a> {
    reference: OriginalArtifactRefV1,
    bytes: &'a [u8],
}
impl BoundedOriginalSource for RawOriginalSource<'_> {
    type Work = ();
    type Receipt = OriginalArtifactRefV1;
    const POOL_ERROR: &'static str = "GEN artifact original source pool";
    const LANE_ERROR: &'static str = "GEN artifact original lane increased";
    fn persist<C: FnMut() -> bool>(
        &self,
        _memory: &MemoryBudget,
        _work: &mut (),
        cancelled: &mut C,
        _max_bytes: usize,
        originals: &mut Originals<'_, '_, '_>,
        _debit: &dyn Fn(u64, u64) -> Result<(), BodyMutationError>,
    ) -> Result<Self::Receipt, BodyMutationError> {
        let length = u64::try_from(self.bytes.len())
            .map_err(|_| BodyMutationError::Invalid("IWS original byte length"))?;
        if self.reference.byte_len != length {
            return Err(BodyMutationError::Invalid("GEN artifact original length"));
        }
        originals.raw_with_poll(self.reference.id, self.bytes, Some(cancelled))?;
        Ok(self.reference)
    }
    fn reference(receipt: Self::Receipt) -> OriginalArtifactRefV1 {
        receipt
    }
}

impl Originals<'_, '_, '_> {
    fn counters(&self) -> [[u64; 7]; 3] {
        std::array::from_fn(|i| {
            [
                self.physical[i].reads_remaining,
                self.physical[i].writes_remaining,
                self.logical[i].nodes_remaining,
                self.physical[i].read_bytes_remaining,
                self.physical[i].write_bytes_remaining,
                self.logical[i].read_bytes_remaining,
                self.logical[i].write_bytes_remaining,
            ]
        })
    }
    fn install(&mut self, values: &[[u64; 7]; 3]) {
        for (i, v) in values.iter().enumerate() {
            self.physical[i].reads_remaining = v[0];
            self.physical[i].writes_remaining = v[1];
            self.logical[i].nodes_remaining = v[2];
            self.physical[i].read_bytes_remaining = v[3];
            self.physical[i].write_bytes_remaining = v[4];
            self.logical[i].read_bytes_remaining = v[5];
            self.logical[i].write_bytes_remaining = v[6];
        }
    }

    /// The complete measurement + begin + stream + finish attempt stays under
    /// one child. A count refusal after measurement still spends that real work.
    /// Targets retain their own pools; only the actual source uses Common's pool.
    pub(super) fn event_bounded(
        &mut self,
        event: &oasis7_world_state::WorldEvent,
        canonical: &mut oasis7_wasm_abi::CanonicalCborWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<OriginalArtifactRefV1, BodyMutationError> {
        self.source_bounded(event, canonical, common)
    }

    pub(super) fn snapshot_bounded(
        &mut self,
        snapshot: &oasis7_world_state::snapshot::SnapshotRecord,
        canonical: &mut CanonicalCborWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<OriginalArtifactRefV1, BodyMutationError> {
        self.source_bounded(snapshot, canonical, common)
    }

    pub(super) fn hot_tick_bounded(
        &mut self,
        tick: &oasis7_world_state::consensus::TickConsensusRecord,
        canonical: &mut CanonicalCborWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<OriginalArtifactRefV1, BodyMutationError> {
        self.source_bounded(tick, canonical, common)
    }

    pub(super) fn retained_audit_bounded(
        &mut self,
        audit: &oasis7_world_state::consensus::TickConsensusRejectionAuditEvent,
        canonical: &mut CanonicalCborWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<OriginalArtifactRefV1, BodyMutationError> {
        self.source_bounded(audit, canonical, common)
    }

    pub(super) fn raw_bounded(
        &mut self,
        reference: OriginalArtifactRefV1,
        bytes: &[u8],
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<OriginalArtifactRefV1, BodyMutationError> {
        // This new source/result/poll frame is separate from EventMeterFrame.
        // Eight fixed operations: construction, length check, six endpoint polls.
        let frame_bytes = std::mem::size_of::<RawOriginalSource<'_>>()
            + 3 * std::mem::size_of::<OriginalArtifactRefV1>()
            + std::mem::size_of::<Option<&mut dyn FnMut() -> bool>>()
            + 4 * std::mem::size_of::<usize>()
            + std::mem::size_of::<MemoryReservation>()
            + std::mem::size_of::<Result<OriginalArtifactRefV1, BodyMutationError>>()
            + std::mem::size_of::<Result<(), BodyMutationError>>();
        common
            .charge(8, frame_bytes as u64)
            .map_err(context_error)?;
        let _frame_fee = common
            .memory()
            .reserve(frame_bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        self.source_bounded(&RawOriginalSource { reference, bytes }, &mut (), common)
    }

    fn source_bounded<S: BoundedOriginalSource>(
        &mut self,
        source: &S,
        canonical: &mut S::Work,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<OriginalArtifactRefV1, BodyMutationError> {
        common.consume_steps(1).map_err(context_error)?;
        if !self.stores[0].uses_budget(common.memory()) {
            return Err(BodyMutationError::Invalid(S::POOL_ERROR));
        }
        // Seven fixed passes over the 21 real IO scalars: snapshot, cap,
        // install, snapshot, validate, reconcile, install. Two codec scalars
        // are reconciled once. Reserve the actual live adapter frame by type.
        let frame_bytes = std::mem::size_of::<EventMeterFrame<'_>>();
        common
            .charge(7 * (3 * 7) + 2, frame_bytes as u64)
            .map_err(context_error)?;
        let _frame_fee = common
            .memory()
            .reserve(frame_bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let (memory, work, stop) = common.parts();
        let lease = work.begin_child().map_err(context_error)?;
        // Nine IO step lanes + codec; twelve IO byte lanes + codec. These
        // limits partition this very lease, without debiting unused allowance.
        let limits = [
            lease.remaining_steps() / 10,
            lease.remaining_read_bytes() / 13,
        ];
        let mut frame = EventMeterFrame {
            before: self.counters(),
            capped: [[0; 7]; 3],
            after: [[0; 7]; 3],
            used: [0; 2],
            codec: CodecShare {
                initial: limits,
                remaining: Cell::new(limits),
                stop: RefCell::new(stop),
                cancelled: Cell::new(false),
            },
            lease,
        };
        for i in 0..3 {
            for j in 0..7 {
                frame.capped[i][j] = frame.before[i][j].min(limits[usize::from(j >= 3)]);
            }
        }
        self.install(&frame.capped);
        let cancelled = self.cancelled;
        let max_bytes = self.max_bytes;
        let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            source.persist(
                memory,
                canonical,
                &mut || {
                    cancelled.load(std::sync::atomic::Ordering::Acquire) || frame.codec.cancelled()
                },
                max_bytes,
                self,
                &|steps, bytes| frame.codec.charge(steps, bytes),
            )
        })) {
            Ok(result) => result,
            Err(payload) => {
                self.install(&[[0; 7]; 3]);
                drop(frame); // abandoned ChildLease drains the original Core
                std::panic::resume_unwind(payload)
            }
        };
        frame.after = self.counters();
        if (0..3).any(|i| (0..7).any(|j| frame.after[i][j] > frame.capped[i][j])) {
            self.install(&[[0; 7]; 3]);
            drop(frame);
            return Err(BodyMutationError::Invalid(S::LANE_ERROR));
        }
        for i in 0..3 {
            for j in 0..7 {
                let used = frame.capped[i][j] - frame.after[i][j];
                // Each lane is bounded by its original lease share, so the
                // nine/twelve IO sums plus the codec share cannot overflow.
                frame.used[usize::from(j >= 3)] += used;
                frame.after[i][j] = frame.before[i][j] - used;
            }
        }
        self.install(&frame.after);
        let remaining = frame.codec.remaining.get();
        for (i, remaining) in remaining.into_iter().enumerate() {
            frame.used[i] += frame.codec.initial[i] - remaining;
        }
        let debit = frame
            .lease
            .consume(frame.used[0], frame.used[1])
            .map_err(context_error);
        frame.lease.finish();
        let receipt = match result {
            Err(BodyMutationError::Value(ValueTreeError::Cancelled))
                if frame.codec.cancelled.get() =>
            {
                return Err(context_error(FormalBoundedChargeErrorV1::Cancelled.into()));
            }
            Err(error) => return Err(error),
            Ok(receipt) => {
                debit?;
                receipt
            }
        };
        Ok(S::reference(receipt))
    }

    fn begin(&mut self, length: u64) -> Result<(), BodyMutationError> {
        if self.cancelled.load(std::sync::atomic::Ordering::Acquire) {
            return Err(ValueTreeError::Cancelled.into());
        }
        if self.error.borrow().is_some() {
            return Err(BodyMutationError::Invalid("IWS original transfer poisoned"));
        }
        if length
            > u64::try_from(self.max_bytes)
                .map_err(|_| BodyMutationError::Invalid("IWS original maximum"))?
        {
            return Err(BodyMutationError::Invalid(
                "IWS original transfer byte bound",
            ));
        }
        *self.occurrences = self
            .occurrences
            .checked_sub(1)
            .ok_or(BodyMutationError::Invalid(
                "IWS original transfer occurrences",
            ))?;
        Ok(())
    }
    pub(super) fn raw(&mut self, id: [u8; 32], bytes: &[u8]) -> Result<(), BodyMutationError> {
        self.raw_with_poll(id, bytes, None)
    }
    fn raw_with_poll(
        &mut self,
        id: [u8; 32],
        bytes: &[u8],
        mut poll: Option<&mut dyn FnMut() -> bool>,
    ) -> Result<(), BodyMutationError> {
        let length = u64::try_from(bytes.len())
            .map_err(|_| BodyMutationError::Invalid("IWS original byte length"))?;
        self.begin(length)?;
        for (index, store) in self.stores.iter().enumerate() {
            if let Some(poll) = poll.as_mut()
                && poll()
            {
                return Err(ValueTreeError::Cancelled.into());
            }
            let mut io = ObjectIo {
                physical: &mut self.physical[index],
                logical: &mut self.logical[index],
                class: MemoryClass::Ordinary,
                cancelled: self.cancelled,
            };
            store
                .write_object(&mut &bytes[..], length, Some(id), &mut io)
                .map_err(|e| store_error(e, self.error))?;
            // The actual writer's first error returns above. Only a successful
            // complete endpoint can be followed by a Common cancellation poll.
            if let Some(poll) = poll.as_mut()
                && poll()
            {
                return Err(ValueTreeError::Cancelled.into());
            }
        }
        Ok(())
    }
    pub(super) fn stream(
        &mut self,
        source: &mut dyn ActionOriginalStreamV1,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<(), BodyMutationError> {
        let length = source.original_bytes();
        let id = source.original_id();
        self.begin(length)?;
        let [sp, pp, rp] = &mut *self.physical;
        let [sl, pl, rl] = &mut *self.logical;
        let mut source_io = ObjectIo {
            physical: sp,
            logical: sl,
            class: MemoryClass::Ordinary,
            cancelled: self.cancelled,
        };
        let mut primary_io = ObjectIo {
            physical: pp,
            logical: pl,
            class: MemoryClass::Ordinary,
            cancelled: self.cancelled,
        };
        let mut replica_io = ObjectIo {
            physical: rp,
            logical: rl,
            class: MemoryClass::Ordinary,
            cancelled: self.cancelled,
        };
        let stop = RefCell::new(cancelled);
        self.stores[0]
            .write_object_from_stream(
                length,
                id,
                &mut source_io,
                &mut || (*stop.borrow_mut())(),
                |source_out| {
                    self.stores[1]
                        .write_object_from_stream(
                            length,
                            id,
                            &mut primary_io,
                            &mut || (*stop.borrow_mut())(),
                            |primary_out| {
                                self.stores[2]
                                    .write_object_from_stream(
                                        length,
                                        id,
                                        &mut replica_io,
                                        &mut || (*stop.borrow_mut())(),
                                        |replica_out| {
                                            source.write_to(&mut Tee {
                                                writers: [
                                                    &mut *source_out,
                                                    &mut *primary_out,
                                                    &mut *replica_out,
                                                ],
                                                failed: false,
                                            })
                                        },
                                    )
                                    .map_err(|e| stream_error(e, self.error))?;
                                Ok(())
                            },
                        )
                        .map_err(|e| stream_error(e, self.error))?;
                    Ok(())
                },
            )
            .map_err(|e| stream_error(e, self.error))?;
        Ok(())
    }
}
