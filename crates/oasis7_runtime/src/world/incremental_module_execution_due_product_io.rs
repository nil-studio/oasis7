//! Common-meter adapter for the real due-product invocation context.
//!
//! The adapter lends one child of the caller's existing Common meter to the
//! legacy executor. It temporarily caps every decrementing Root-module lane,
//! then reconciles the exact deltas back to the caller's original budgets.
//! Limits such as maximum input/state/owner/heap/event sizes, policy counts,
//! member limits, selection text limits, and worker memory/output/wall-time
//! limits remain limits; they are not copied into fresh work counters here.

use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction},
    incremental_material::{OriginalObjectReader, OriginalObjectStreamReader},
    incremental_state::{MerkleNodeReader, StatePageWriter, StateScanBudget},
    incremental_value::{ValueIoBudget, ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use oasis7_wasm_executor::bounded_worker::{
    BoundedWorkerSandboxV1, WorkerExecutableV1, WorkerLimitsV1,
};
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_value_mutation::ValueMutationContextV1,
};
use std::{
    cell::RefCell,
    io::{self, Write},
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    sync::atomic::{AtomicBool, Ordering},
};

const MAX_ROOT_LANES: usize = 128;

type StopFn<'a> = &'a mut (dyn FnMut() -> bool + 'a);
type StateErrorFn<'a> = &'a mut (dyn FnMut() -> Option<ValueTreeError> + 'a);
type PersistOriginalFn<'a> = &'a mut (
            dyn FnMut(
    &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
) -> Result<(), BodyMutationError>
                + 'a
        );

type ReborrowedRoot<'a, 's, M, R, W, O, C> = RootModuleExecutionContextV1<
    'a,
    'a,
    's,
    'a,
    M,
    R,
    StateErrorFn<'a>,
    StopFn<'a>,
    W,
    PersistOriginalFn<'a>,
    BorrowedOriginals<'a, 'a, O, C>,
>;

enum LaneMut<'a> {
    Steps(&'a mut u64),
    Bytes(&'a mut u64),
    Entries(&'a mut usize),
}

#[derive(Clone, Copy)]
struct LaneFrameV1 {
    values: [u128; MAX_ROOT_LANES],
    count: usize,
    step_lanes: u64,
    byte_lanes: u64,
}

impl LaneFrameV1 {
    fn capture<M, R, E, C, W, F, O>(
        root: &mut RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>,
    ) -> Result<Self, ValueTreeError>
    where
        O: OriginalObjectStreamReader,
    {
        let mut frame = Self {
            values: [0; MAX_ROOT_LANES],
            count: 0,
            step_lanes: 0,
            byte_lanes: 0,
        };
        visit_lanes(root, &mut |lane| {
            let Some(slot) = frame.values.get_mut(frame.count) else {
                frame.count = MAX_ROOT_LANES + 1;
                return;
            };
            match lane {
                LaneMut::Steps(value) => {
                    *slot = u128::from(*value);
                    frame.step_lanes += 1;
                }
                LaneMut::Bytes(value) => {
                    *slot = u128::from(*value);
                    frame.byte_lanes += 1;
                }
                LaneMut::Entries(value) => {
                    *slot = *value as u128;
                    frame.step_lanes += 1;
                }
            }
            frame.count += 1;
        });
        if frame.count > MAX_ROOT_LANES {
            return Err(ValueTreeError::ResourceLimited(
                "Root module Common lane inventory",
            ));
        }
        Ok(frame)
    }
}

fn visit_scan(scan: &mut StateScanBudget, visit: &mut impl for<'a> FnMut(LaneMut<'a>)) {
    visit(LaneMut::Steps(&mut scan.nodes_remaining));
    visit(LaneMut::Bytes(&mut scan.read_bytes_remaining));
    visit(LaneMut::Steps(&mut scan.leaves_remaining));
}

fn visit_io(io: &mut ValueIoBudget, visit: &mut impl for<'a> FnMut(LaneMut<'a>)) {
    visit(LaneMut::Steps(&mut io.nodes_remaining));
    visit(LaneMut::Bytes(&mut io.read_bytes_remaining));
    visit(LaneMut::Bytes(&mut io.write_bytes_remaining));
}

/// Enumerates actual decrementing fields, not the unrelated fixed lane count
/// used by another family. The inventory is repeated only at cap/reconcile
/// boundaries; each traversal holds at most one mutable lane reference.
fn visit_lanes<M, R, E, C, W, F, O>(
    root: &mut RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>,
    visit: &mut impl for<'a> FnMut(LaneMut<'a>),
) where
    O: OriginalObjectStreamReader,
{
    let work = &mut *root.work;
    visit(LaneMut::Steps(&mut work.input.visits_remaining));
    visit(LaneMut::Bytes(&mut work.input.bytes_remaining));
    visit(LaneMut::Steps(&mut work.routing.nodes_remaining));
    visit(LaneMut::Steps(&mut work.routing.edges_remaining));
    visit(LaneMut::Bytes(&mut work.routing.string_bytes_remaining));
    visit(LaneMut::Steps(&mut work.routing.match_steps_remaining));
    visit(LaneMut::Steps(&mut work.lookup_steps_remaining));
    visit_scan(&mut work.anchor_scan, visit);
    visit(LaneMut::Steps(&mut work.anchor_cells_remaining));
    visit(LaneMut::Entries(&mut work.visual.pending_entries_remaining));
    visit_scan(&mut work.visual.body_scan, visit);
    visit(LaneMut::Steps(&mut work.visual.cells_remaining));
    visit(LaneMut::Steps(&mut work.visual_decode.nodes_remaining));
    visit(LaneMut::Bytes(&mut work.visual_decode.text_bytes_remaining));
    visit(LaneMut::Steps(&mut work.effects.clone_nodes_remaining));
    visit(LaneMut::Bytes(&mut work.effects.clone_bytes_remaining));
    visit(LaneMut::Steps(&mut work.effect_records.cells_remaining));
    visit(LaneMut::Steps(
        &mut work.effect_records.json_nodes_remaining,
    ));
    visit(LaneMut::Steps(&mut work.authorization.cells_remaining));
    visit(LaneMut::Bytes(
        &mut work.authorization.comparison_bytes_remaining,
    ));
    visit(LaneMut::Steps(&mut work.output.calls_remaining));
    visit(LaneMut::Bytes(&mut work.output.structure.bytes_remaining));
    visit(LaneMut::Steps(&mut work.output.structure.values_remaining));
    visit_scan(&mut work.selection_scan, visit);
    visit(LaneMut::Steps(&mut work.selection.cells_remaining));
    visit(LaneMut::Steps(&mut work.selection.json.cells_remaining));
    visit(LaneMut::Steps(
        &mut work.selection.json.json_nodes_remaining,
    ));
    visit(LaneMut::Bytes(
        &mut work.selection_limits.manifest_json_bytes_remaining,
    ));
    visit(LaneMut::Steps(&mut work.original.objects_remaining));
    visit(LaneMut::Bytes(&mut work.original.bytes_remaining));

    visit_scan(&mut root.manifest_work.body_scan, visit);
    visit(LaneMut::Steps(
        &mut root.manifest_work.fields.cells_remaining,
    ));
    visit(LaneMut::Steps(
        &mut root.manifest_work.fields.json.cells_remaining,
    ));
    visit(LaneMut::Steps(
        &mut root.manifest_work.fields.json.json_nodes_remaining,
    ));
    visit(LaneMut::Steps(&mut root.manifest_work.json.cells_remaining));
    visit(LaneMut::Steps(
        &mut root.manifest_work.json.json_nodes_remaining,
    ));
    visit(LaneMut::Bytes(&mut root.manifest_work.hash_bytes_remaining));

    // The root invocation path has no separate invocation-enumeration counter;
    // selection scans and closures are represented by the lanes above.

    visit_io(&mut root.ctx.values.io, visit);
    visit(LaneMut::Bytes(&mut root.ctx.key_work.bytes_remaining));
    visit_io(&mut root.members.io, visit);
    visit(LaneMut::Bytes(&mut root.members.key_work.bytes_remaining));
    visit_io(&mut root.fees.values.io, visit);
    visit(LaneMut::Bytes(&mut root.fees.key_work.bytes_remaining));
    visit_io(&mut root.events.mutation.io, visit);
    visit(LaneMut::Steps(
        &mut root.events.original_work.visits_remaining,
    ));
    visit(LaneMut::Bytes(
        &mut root.events.original_work.bytes_remaining,
    ));
}

fn cap_lanes<M, R, E, C, W, F, O>(
    root: &mut RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>,
    frame: LaneFrameV1,
    steps: u64,
    bytes: u64,
) -> Result<(), ValueTreeError>
where
    O: OriginalObjectStreamReader,
{
    let entry_cap = usize::try_from(steps).unwrap_or(usize::MAX);
    let mut index = 0usize;
    visit_lanes(root, &mut |lane| {
        index = index.saturating_add(1);
        match lane {
            LaneMut::Steps(value) => *value = (*value).min(steps),
            LaneMut::Bytes(value) => *value = (*value).min(bytes),
            LaneMut::Entries(value) => *value = (*value).min(entry_cap),
        }
    });
    if index != frame.count {
        return Err(ValueTreeError::Invalid(
            "Root module Common lane inventory changed",
        ));
    }
    Ok(())
}

struct LaneDebitV1 {
    steps: u64,
    bytes: u64,
    invalid: bool,
}

fn reconcile_lanes<M, R, E, C, W, F, O>(
    root: &mut RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>,
    frame: LaneFrameV1,
    step_quota: u64,
    byte_quota: u64,
) -> Result<LaneDebitV1, ValueTreeError>
where
    O: OriginalObjectStreamReader,
{
    let step_quota_u128 = u128::from(step_quota);
    let byte_quota_u128 = u128::from(byte_quota);
    let entry_quota = usize::try_from(step_quota).unwrap_or(usize::MAX) as u128;
    let mut index = 0usize;
    let mut used_steps = 0u128;
    let mut used_bytes = 0u128;
    let mut invalid = false;
    visit_lanes(root, &mut |lane| {
        let original = frame.values.get(index).copied().unwrap_or(0);
        index = index.saturating_add(1);
        let (quota, actual) = match lane {
            LaneMut::Steps(value) => {
                let quota = original.min(step_quota_u128);
                let actual = u128::from(*value);
                let used = if actual > quota {
                    invalid = true;
                    quota
                } else {
                    quota - actual
                };
                used_steps = used_steps.saturating_add(used);
                *value = u64::try_from(original - used).unwrap_or(0);
                (quota, actual)
            }
            LaneMut::Bytes(value) => {
                let quota = original.min(byte_quota_u128);
                let actual = u128::from(*value);
                let used = if actual > quota {
                    invalid = true;
                    quota
                } else {
                    quota - actual
                };
                used_bytes = used_bytes.saturating_add(used);
                *value = u64::try_from(original - used).unwrap_or(0);
                (quota, actual)
            }
            LaneMut::Entries(value) => {
                let quota = original.min(entry_quota);
                let actual = *value as u128;
                let used = if actual > quota {
                    invalid = true;
                    quota
                } else {
                    quota - actual
                };
                used_steps = used_steps.saturating_add(used);
                *value = usize::try_from(original - used).unwrap_or(0);
                (quota, actual)
            }
        };
        if actual > quota {
            invalid = true;
        }
    });
    if index != frame.count {
        invalid = true;
    }
    let steps = u64::try_from(used_steps)
        .map_err(|_| ValueTreeError::ResourceLimited("Root module Common step debit"))?;
    let bytes = u64::try_from(used_bytes)
        .map_err(|_| ValueTreeError::ResourceLimited("Root module Common byte debit"))?;
    Ok(LaneDebitV1 {
        steps,
        bytes,
        invalid,
    })
}

#[derive(Clone, Copy)]
enum CancelSourceV1 {
    Overlay,
    Member,
    Fee,
    Event,
    Original,
}

struct CancelMuxV1<'a, C> {
    common: &'a mut dyn FnMut() -> bool,
    overlay: &'a mut C,
    member: &'a mut C,
    fee: &'a mut C,
    event: &'a mut C,
    worker: &'a AtomicBool,
    latched: bool,
}

impl<C: FnMut() -> bool> CancelMuxV1<'_, C> {
    fn poll_common(&mut self) -> bool {
        let stopped = (self.common)() || self.worker.load(Ordering::Relaxed);
        self.latched |= stopped;
        if stopped {
            self.worker.store(true, Ordering::Relaxed);
        }
        stopped
    }

    fn poll(&mut self, source: CancelSourceV1) -> bool {
        let common = (self.common)();
        let local = match source {
            CancelSourceV1::Overlay | CancelSourceV1::Original => (self.overlay)(),
            CancelSourceV1::Member => (self.member)(),
            CancelSourceV1::Fee => (self.fee)(),
            CancelSourceV1::Event => (self.event)(),
        };
        let stopped = common | local | self.worker.load(Ordering::Relaxed);
        self.latched |= stopped;
        if stopped {
            self.worker.store(true, Ordering::Relaxed);
        }
        stopped
    }

    fn poll_all(&mut self) -> bool {
        let common = (self.common)();
        let overlay = (self.overlay)();
        let member = (self.member)();
        let fee = (self.fee)();
        let event = (self.event)();
        let worker = self.worker.load(Ordering::Relaxed);
        let stopped = common | overlay | member | fee | event | worker;
        self.latched |= stopped;
        if stopped {
            self.worker.store(true, Ordering::Relaxed);
        }
        stopped
    }
}

pub(super) struct BorrowedOriginals<'a, 'cancel, O, C> {
    source: &'a mut O,
    cancellation: &'a RefCell<CancelMuxV1<'cancel, C>>,
}

struct CancelledWriter<'a, 'cancel, C> {
    target: &'a mut dyn Write,
    cancellation: &'a RefCell<CancelMuxV1<'cancel, C>>,
}

impl<C: FnMut() -> bool> Write for CancelledWriter<'_, '_, C> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self
            .cancellation
            .borrow_mut()
            .poll(CancelSourceV1::Original)
        {
            return Err(io::Error::other("due product original cancelled"));
        }
        self.target.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        if self
            .cancellation
            .borrow_mut()
            .poll(CancelSourceV1::Original)
        {
            return Err(io::Error::other("due product original cancelled"));
        }
        self.target.flush()
    }
}

impl<O: OriginalObjectReader, C: FnMut() -> bool> OriginalObjectReader
    for BorrowedOriginals<'_, '_, O, C>
{
    type Error = O::Error;

    fn allocation_budget(&self) -> &MemoryBudget {
        self.source.allocation_budget()
    }
    fn allocation_class(&self) -> MemoryClass {
        self.source.allocation_class()
    }
    fn read_original(
        &mut self,
        id: &[u8; 32],
        max_bytes: usize,
        memory: &MemoryBudget,
    ) -> Result<std::sync::Arc<oasis7_proto::memory_budget::ReservedValue<Vec<u8>>>, Self::Error>
    {
        let result = self.source.read_original(id, max_bytes, memory);
        // The trait has no cancellation error type. Latch cancellation after
        // the bounded source call so the enclosing paid operation rejects a
        // successful read without replacing a source error.
        let _ = self
            .cancellation
            .borrow_mut()
            .poll(CancelSourceV1::Original);
        result
    }
}

impl<O: OriginalObjectStreamReader, C: FnMut() -> bool> OriginalObjectStreamReader
    for BorrowedOriginals<'_, '_, O, C>
{
    fn stream_original(
        &mut self,
        id: &[u8; 32],
        max_bytes: u64,
        memory: &MemoryBudget,
        output: &mut dyn Write,
    ) -> Result<u64, Self::Error> {
        let mut output = CancelledWriter {
            target: output,
            cancellation: self.cancellation,
        };
        let result = self
            .source
            .stream_original(id, max_bytes, memory, &mut output);
        // Also observe cancellation for empty streams, where the writer is
        // never called. Preserve the source's first returned error.
        let _ = self
            .cancellation
            .borrow_mut()
            .poll(CancelSourceV1::Original);
        result
    }
}

fn root_invalid<OE>(message: &'static str) -> RootModuleExecutionErrorV1<OE> {
    ValueTreeError::Invalid(message).into()
}

fn root_formal_error<OE>(
    error: impl std::error::Error + Send + Sync + 'static,
) -> RootModuleExecutionErrorV1<OE> {
    RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(error)))
}

fn abort_preserving_first<M: MerkleNodeReader>(transaction: &mut ExecutionTransaction<'_, M>) {
    let _: Result<(), ExecutionError> = transaction.contribute(|_| Err(ExecutionError::Aborted));
}

#[allow(clippy::too_many_arguments)]
fn run_reborrowed<'a, 's, M, R, E, C, W, F, O, T, Run>(
    root: &mut RootModuleExecutionContextV1<'a, '_, 's, '_, M, R, E, C, W, F, O>,
    common_stop: &mut dyn FnMut() -> bool,
    live_owner: &mut dyn FnMut() -> ExecutionFence,
    run: Run,
) -> Result<(T, bool), RootModuleExecutionErrorV1<O::Error>>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
    T: 'static,
    Run: for<'b> FnOnce(
        &mut ReborrowedRoot<'b, 's, M, R, W, O, C>,
        &mut (dyn FnMut() -> ExecutionFence + 'b),
    ) -> Result<T, RootModuleExecutionErrorV1<O::Error>>,
{
    let RootModuleExecutionContextV1 {
        families,
        now,
        state,
        ctx,
        members,
        fees,
        events,
        originals,
        worker,
        manifest_cache,
        manifest_work,
        work,
        #[cfg(any(test, feature = "test-support"))]
        input_observation,
    } = root;
    let WorldFieldOverlayContextV1 {
        transaction,
        values,
        key_work,
        semantic_bytes,
        record_bytes,
        take_state_error,
        cancelled,
    } = &mut **ctx;
    let oasis7_world_state::incremental_field_member::MemberRecordWriteContext {
        sink: member_sink,
        memory: member_memory,
        io: member_io,
        key_work: member_key_work,
        limits: member_limits,
        cancelled: member_cancelled,
    } = &mut **members;
    let RootModuleFeeWriteContextV1 {
        values: fee_values,
        key_work: fee_key_work,
        member_limits: fee_member_limits,
    } = &mut **fees;
    let ValueMutationContextV1 {
        sink: fee_sink,
        memory: fee_memory,
        io: fee_io,
        cancelled: fee_cancelled,
    } = fee_values;
    let ModuleOutputEventContextV1 {
        values: event_values,
        world_events,
        mutation: event_mutation,
        persist_original,
        max_event_bytes,
        original_work,
    } = &mut **events;
    let BodyMutationContext {
        sink: event_sink,
        memory: event_memory,
        io: event_io,
        cancelled: event_cancelled,
    } = event_mutation;

    let worker_cancelled = worker.cancelled;
    let cancellation = RefCell::new(CancelMuxV1 {
        common: common_stop,
        overlay: cancelled,
        member: member_cancelled,
        fee: fee_cancelled,
        event: event_cancelled,
        worker: worker_cancelled,
        latched: false,
    });

    let mut overlay_cancelled = || cancellation.borrow_mut().poll(CancelSourceV1::Overlay);
    let mut member_cancelled = || cancellation.borrow_mut().poll(CancelSourceV1::Member);
    let mut fee_cancelled = || cancellation.borrow_mut().poll(CancelSourceV1::Fee);
    let mut event_cancelled = || cancellation.borrow_mut().poll(CancelSourceV1::Event);
    let mut take_error = || {
        let existing = take_state_error();
        if existing.is_some() {
            existing
        } else if cancellation.borrow_mut().poll(CancelSourceV1::Overlay) {
            Some(ValueTreeError::Cancelled)
        } else {
            None
        }
    };
    let mut persist =
        |original: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
            if cancellation.borrow_mut().poll(CancelSourceV1::Event) {
                return Err(ValueTreeError::Cancelled.into());
            }
            persist_original(original)
        };

    let mut overlay_cancelled_ref: StopFn<'_> = &mut overlay_cancelled;
    let mut member_cancelled_ref: StopFn<'_> = &mut member_cancelled;
    let mut fee_cancelled_ref: StopFn<'_> = &mut fee_cancelled;
    let mut event_cancelled_ref: StopFn<'_> = &mut event_cancelled;
    let mut take_error_ref: StateErrorFn<'_> = &mut take_error;
    let mut persist_ref: PersistOriginalFn<'_> = &mut persist;

    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: &mut **transaction,
        values: oasis7_formal_protocol::incremental_value::ValueReadContext {
            source: values.source,
            memory: values.memory,
            class: values.class,
            io: &mut *values.io,
        },
        key_work: &mut **key_work,
        semantic_bytes: *semantic_bytes,
        record_bytes: *record_bytes,
        take_state_error: take_error_ref,
        cancelled: overlay_cancelled_ref,
    };
    let mut member = MemberRecordWriteContext {
        sink: &mut **member_sink,
        memory: *member_memory,
        io: &mut **member_io,
        key_work: &mut **member_key_work,
        limits: *member_limits,
        cancelled: member_cancelled_ref,
    };
    let mut fee = RootModuleFeeWriteContextV1 {
        values: ValueMutationContextV1 {
            sink: &mut **fee_sink,
            memory: *fee_memory,
            io: &mut **fee_io,
            cancelled: fee_cancelled_ref,
        },
        key_work: &mut **fee_key_work,
        member_limits: *fee_member_limits,
    };
    let placeholder = CanonicalCborWorkV1 {
        visits_remaining: 0,
        bytes_remaining: 0,
        max_depth: original_work.max_depth,
        max_map_keys: original_work.max_map_keys,
        max_key_bytes: original_work.max_key_bytes,
    };
    let mut event = ModuleOutputEventContextV1 {
        values: *event_values,
        world_events: *world_events,
        mutation: BodyMutationContext {
            sink: &mut **event_sink,
            memory: *event_memory,
            io: &mut **event_io,
            cancelled: event_cancelled_ref,
        },
        persist_original: persist_ref,
        max_event_bytes: *max_event_bytes,
        original_work: std::mem::replace(original_work, placeholder),
    };
    let executable = match &worker.executable {
        WorkerExecutableV1::SealedFd(file) => WorkerExecutableV1::SealedFd(file),
        WorkerExecutableV1::NativeBoundSealedFd { file, descriptor } => {
            WorkerExecutableV1::NativeBoundSealedFd { file, descriptor }
        }
        WorkerExecutableV1::FixturePath(path) => WorkerExecutableV1::FixturePath(path),
    };
    let mut backend = BoundedWorkerSandboxV1 {
        executable,
        allocator: worker.allocator,
        cancelled: worker_cancelled,
        limits: WorkerLimitsV1 {
            address_space_bytes: worker.limits.address_space_bytes,
            max_request_bytes: worker.limits.max_request_bytes,
            max_output_bytes: worker.limits.max_output_bytes,
            wall_time_ms: worker.limits.wall_time_ms,
        },
    };
    let mut borrowed_originals = BorrowedOriginals {
        source: &mut **originals,
        cancellation: &cancellation,
    };
    let mut live = || {
        let _ = cancellation.borrow_mut().poll_common();
        live_owner()
    };
    let result = {
        let mut actual: ReborrowedRoot<'_, 's, M, R, W, O, C> = RootModuleExecutionContextV1 {
            families: *families,
            now: *now,
            state: *state,
            ctx: &mut overlay,
            members: &mut member,
            fees: &mut fee,
            events: &mut event,
            originals: &mut borrowed_originals,
            worker: &mut backend,
            manifest_cache: *manifest_cache,
            manifest_work: &mut **manifest_work,
            work: &mut **work,
            #[cfg(any(test, feature = "test-support"))]
            input_observation: input_observation.as_deref_mut(),
        };
        let result = run(&mut actual, &mut live);
        let original_visits_remaining = actual.events.original_work.visits_remaining;
        let original_bytes_remaining = actual.events.original_work.bytes_remaining;
        drop(actual);
        (result, original_visits_remaining, original_bytes_remaining)
    };
    // The temporary event context owns the original-work counters while the
    // invocation runs. Reconcile only the two mutable counters; the copied
    // limits are unchanged and remain owned by the outer context.
    let (result, original_visits_remaining, original_bytes_remaining) = result;
    original_work.visits_remaining = original_visits_remaining;
    original_work.bytes_remaining = original_bytes_remaining;
    let stopped_after = cancellation.borrow_mut().poll_all();
    let result = match result {
        Err(error) => Err(error),
        Ok(_value) if stopped_after => Err(ValueTreeError::Cancelled.into()),
        Ok(value) => Ok(value),
    };
    let latched = cancellation.borrow().latched;
    Ok((result?, latched))
}

/// Run an actual invocation under one exclusive Common child. The result type
/// must own its data; it cannot retain the temporary reborrowed context.
#[allow(clippy::too_many_arguments)]
pub(super) fn paid<'a, 't, 's, 'v, M, R, E, C, W, F, O, T, Run>(
    root: &mut RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>,
    common: &mut FormalBoundedContextV1<'_>,
    live_owner: &mut dyn FnMut() -> ExecutionFence,
    run: Run,
) -> Result<T, RootModuleExecutionErrorV1<O::Error>>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
    T: 'static,
    Run: for<'b> FnOnce(
        &mut ReborrowedRoot<'b, 's, M, R, W, O, C>,
        &mut (dyn FnMut() -> ExecutionFence + 'b),
    ) -> Result<T, RootModuleExecutionErrorV1<O::Error>>,
{
    let result = (|| {
        let memory = common.memory();
        if !root.ctx.transaction.uses_budget(memory)
            || !root.ctx.values.memory.same_pool(memory)
            || root.ctx.values.class != MemoryClass::Ordinary
            || !root.members.memory.same_pool(memory)
            || !root.fees.values.memory.same_pool(memory)
            || !root.events.mutation.memory.same_pool(memory)
            || !root.originals.allocation_budget().same_pool(memory)
            || root.originals.allocation_class() != MemoryClass::Ordinary
            || !root.worker.allocator.uses_budget(memory)
            || !root.manifest_cache.uses_budget(memory)
            || !root.families.parent.uses_budget(memory)
            || !root.families.instances.uses_budget(memory)
            || !root.families.config.uses_budget(memory)
            || !root.families.content.uses_budget(memory)
            || !root.families.agents.uses_budget(memory)
            || !root.families.authorization.uses_budget(memory)
            || !root.families.effects.uses_budget(memory)
            || !root.families.state.uses_budget(memory)
            || !root.families.agent_authority.uses_budget(memory)
            || !root.families.marketplace.uses_budget(memory)
        {
            return Err(root_invalid("due product Common owner pool"));
        }
        let live = live_owner();
        root.ctx
            .transaction
            .validate_guard(&root.ctx.transaction.guard(), &live)?;
        common
            .consume_steps(1)
            .map_err(root_formal_error::<O::Error>)?;

        let result = {
            let (memory, meter, stop) = common.parts();
            let _lane_frame_memory = memory
                .reserve(std::mem::size_of::<LaneFrameV1>(), MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let lease = meter.begin_child().map_err(root_formal_error::<O::Error>)?;
            let frame = LaneFrameV1::capture(root)?;
            if frame.step_lanes == 0 || frame.byte_lanes == 0 {
                return Err(root_invalid("due product Common lane inventory empty"));
            }
            let step_quota = lease.remaining_steps() / frame.step_lanes;
            let byte_quota = lease.remaining_read_bytes() / frame.byte_lanes;
            cap_lanes(root, frame, step_quota, byte_quota)?;

            let execution = catch_unwind(AssertUnwindSafe(|| {
                run_reborrowed(root, stop, live_owner, run)
            }));
            let (result, lane_debit) = match execution {
                Err(payload) => {
                    root.worker.cancelled.store(true, Ordering::Relaxed);
                    drop(lease);
                    abort_preserving_first(root.ctx.transaction);
                    resume_unwind(payload)
                }
                Ok(Err(error)) => {
                    let debit = reconcile_lanes(root, frame, step_quota, byte_quota);
                    (Err(error), debit)
                }
                Ok(Ok((value, cancelled))) => {
                    let debit = reconcile_lanes(root, frame, step_quota, byte_quota);
                    let result = if cancelled {
                        Err(ValueTreeError::Cancelled.into())
                    } else {
                        Ok(value)
                    };
                    (result, debit)
                }
            };
            let debit = match lane_debit {
                Ok(debit) => debit,
                Err(error) => {
                    let _ = lease.consume(u64::MAX, u64::MAX);
                    lease.finish();
                    return Err(error.into());
                }
            };
            let consumed = lease.consume(debit.steps, debit.bytes);
            lease.finish();
            if let Err(error) = consumed {
                return Err(root_formal_error(error));
            }
            if debit.invalid {
                return Err(ValueTreeError::Invalid("due product work lane increased").into());
            }
            result
        };
        let final_poll = common
            .consume_steps(0)
            .map_err(root_formal_error::<O::Error>);
        match (result, final_poll) {
            (Err(RootModuleExecutionErrorV1::Business(_)), Err(error)) => Err(error),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
            (Ok(value), Ok(())) => Ok(value),
        }
    })();
    if matches!(&result, Err(RootModuleExecutionErrorV1::Business(_))) {
        // The existing product owner classifies this live business result
        // after the paid call. It must remain available for that decision.
    } else if result.is_err() {
        abort_preserving_first(root.ctx.transaction);
    }
    result
}
