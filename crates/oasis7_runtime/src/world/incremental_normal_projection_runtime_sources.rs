//! Fixed Runtime aux source adapter. Every fine lane belongs to one Common
//! child, including both real read contexts and the five resource depths.
use crate::world::{
    incremental_body::BodyMutationError, incremental_normal_sources::BoundNormalTickParentV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation};
use oasis7_world_state::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_projection_aux::{
        CurrentRuntimeProjectionAuxV1, RuntimeProjectionAuxWorkV1,
        capture_current_runtime_projection_aux_v1, capture_qualified_runtime_projection_aux_v1,
    },
};
fn bad() -> BodyMutationError {
    BodyMutationError::Invalid("Runtime projection actual Common source")
}
fn formal(e: impl std::error::Error + Send + Sync + 'static) -> BodyMutationError {
    BodyMutationError::Original(Box::new(e))
}
fn overlay(e: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match e {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        WorldFieldOverlayErrorV1::Member(e) => BodyMutationError::Original(Box::new(e)),
    }
}
pub(in crate::world) struct PaidNormalRuntimeProjectionSourcesV1 {
    source: CurrentRuntimeProjectionAuxV1,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
impl PaidNormalRuntimeProjectionSourcesV1 {
    pub(in crate::world) fn source(&self) -> &CurrentRuntimeProjectionAuxV1 {
        &self.source
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        common.consume_steps(1).map_err(formal)?;
        if self.core != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !self._fee.uses_budget(common.memory())
        {
            return Err(bad());
        }
        self.source
            .validate(tx, common.memory(), live)
            .map_err(overlay)
    }
}
#[derive(Clone, Copy)]
enum Scope {
    Current,
    Qualified,
}
#[allow(clippy::too_many_arguments)]
fn capture<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &BoundNormalTickParentV1<'_>,
    read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, V, E, C>,
    work: &mut RuntimeProjectionAuxWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
    scope: Scope,
) -> Result<PaidNormalRuntimeProjectionSourcesV1, BodyMutationError> {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        capture_inner(parent, read, ctx, work, common, live, scope)
    }));
    match outcome {
        Ok(result) => {
            if result.is_err() {
                let _: Result<(), ExecutionError> =
                    ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
            }
            result
        }
        Err(panic) => {
            let _: Result<(), ExecutionError> =
                ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
            std::panic::resume_unwind(panic)
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn capture_inner<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &BoundNormalTickParentV1<'_>,
    read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, V, E, C>,
    work: &mut RuntimeProjectionAuxWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
    scope: Scope,
) -> Result<PaidNormalRuntimeProjectionSourcesV1, BodyMutationError> {
    parent.validate_context(common).map_err(formal)?;
    if ctx.transaction.parent_fence() != parent.fence()
        || !ctx.transaction.uses_budget(common.memory())
        || !ctx.values.memory.same_pool(common.memory())
        || !read.values.memory.same_pool(common.memory())
        || ctx.values.class != MemoryClass::Ordinary
        || read.values.class != MemoryClass::Ordinary
    {
        return Err(bad());
    }
    let bytes = std::mem::size_of::<PaidNormalRuntimeProjectionSourcesV1>() + 4096;
    common.charge(1, bytes as u64).map_err(formal)?;
    let fee = common
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let core = common.work().identity();
    let memory = common.memory().clone();
    macro_rules! steps {
        ($m:ident) => {
            $m!(
                ctx.values.io.nodes_remaining,
                read.values.io.nodes_remaining,
                work.outer_scan.nodes_remaining,
                work.outer_scan.leaves_remaining,
                work.inner_scan.nodes_remaining,
                work.inner_scan.leaves_remaining,
                work.resource_scans[0].nodes_remaining,
                work.resource_scans[0].leaves_remaining,
                work.resource_scans[1].nodes_remaining,
                work.resource_scans[1].leaves_remaining,
                work.resource_scans[2].nodes_remaining,
                work.resource_scans[2].leaves_remaining,
                work.resource_scans[3].nodes_remaining,
                work.resource_scans[3].leaves_remaining,
                work.resource_scans[4].nodes_remaining,
                work.resource_scans[4].leaves_remaining,
                work.config.cells_remaining,
                work.config.json.cells_remaining,
                work.config.json.json_nodes_remaining,
                work.resource_cells.cells_remaining,
                work.governance.cells_remaining,
                work.json.json.cells_remaining,
                work.json.json.json_nodes_remaining,
                work.json.paths_remaining,
                *work.authorization_cells,
                *work.controls_cells,
                work.processing.steps_remaining
            )
        };
    }
    macro_rules! bytes {
        ($m:ident) => {
            $m!(
                ctx.values.io.read_bytes_remaining,
                ctx.values.io.write_bytes_remaining,
                ctx.key_work.bytes_remaining,
                read.values.io.read_bytes_remaining,
                read.values.io.write_bytes_remaining,
                read.key_work.bytes_remaining,
                work.outer_scan.read_bytes_remaining,
                work.inner_scan.read_bytes_remaining,
                work.resource_scans[0].read_bytes_remaining,
                work.resource_scans[1].read_bytes_remaining,
                work.resource_scans[2].read_bytes_remaining,
                work.resource_scans[3].read_bytes_remaining,
                work.resource_scans[4].read_bytes_remaining,
                work.processing.bytes_remaining
            )
        };
    }
    macro_rules! snap{($($lane:expr),*)=>{[$($lane),*]};}
    let original_s = steps!(snap);
    let original_b = bytes!(snap);
    let original_n = *work.pending_entries;
    let (_, meter, stop) = common.parts();
    let child = meter.begin_child().map_err(formal)?;
    let sc = child.remaining_steps() / 28;
    let bc = child.remaining_read_bytes() / 14;
    macro_rules! clamp_s{($($lane:expr),*)=>{$($lane=$lane.min(sc);)*};}
    macro_rules! clamp_b{($($lane:expr),*)=>{$($lane=$lane.min(bc);)*};}
    steps!(clamp_s);
    bytes!(clamp_b);
    *work.pending_entries = (*work.pending_entries).min(usize::try_from(sc).unwrap_or(usize::MAX));
    let before_s = steps!(snap);
    let before_b = bytes!(snap);
    let before_n = *work.pending_entries;
    let guard = ctx.transaction.guard();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let stop = std::cell::RefCell::new(stop);
        let mut read_stop = || (stop.borrow_mut())() || (read.cancelled)();
        let mut current_stop = || (stop.borrow_mut())() || (ctx.cancelled)();
        let mut parent_read = WorldFieldReadContextV1 {
            root: read.root,
            state: read.state,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: read.values.source,
                memory: read.values.memory,
                class: read.values.class,
                io: &mut *read.values.io,
            },
            key_work: &mut *read.key_work,
            semantic_bytes: read.semantic_bytes,
            record_bytes: read.record_bytes,
            take_state_error: &mut read.take_state_error,
            cancelled: &mut read_stop,
        };
        let mut current = WorldFieldOverlayContextV1 {
            transaction: &mut *ctx.transaction,
            values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut current_stop,
        };
        let q = parent.refs().storage.current().runtime();
        let roots = parent.refs().roots;
        match scope {
            Scope::Current => capture_current_runtime_projection_aux_v1(
                q,
                roots,
                &mut parent_read,
                &mut current,
                work,
                &guard,
                live,
            ),
            Scope::Qualified => capture_qualified_runtime_projection_aux_v1(
                q,
                roots,
                &mut parent_read,
                &mut current,
                work,
                &guard,
                live,
            ),
        }
        .map_err(overlay)
    }));
    let after_s = steps!(snap);
    let after_b = bytes!(snap);
    let after_n = *work.pending_entries;
    let debit = |before: &[u64], after: &[u64]| {
        before
            .iter()
            .zip(after)
            .try_fold(0u64, |n, (b, a)| n.checked_add(b.checked_sub(*a)?))
    };
    let used = debit(&before_s, &after_s)
        .and_then(|n| n.checked_add(u64::try_from(before_n.checked_sub(after_n)?).ok()?))
        .zip(debit(&before_b, &after_b));
    let mut i = 0;
    macro_rules! restore_s{($($lane:expr),*)=>{$({$lane=original_s[i].saturating_sub(before_s[i].saturating_sub(after_s[i]));i+=1;})*};}
    steps!(restore_s);
    let _ = i;
    let mut i = 0;
    macro_rules! restore_b{($($lane:expr),*)=>{$({$lane=original_b[i].saturating_sub(before_b[i].saturating_sub(after_b[i]));i+=1;})*};}
    bytes!(restore_b);
    let _ = i;
    *work.pending_entries = original_n.saturating_sub(before_n.saturating_sub(after_n));
    let paid = match used {
        Some((s, b)) => child.consume(s, b).map_err(formal),
        None => Err(bad()),
    };
    child.finish();
    match outcome {
        Err(panic) => {
            let _: Result<(), ExecutionError> =
                ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
            std::panic::resume_unwind(panic)
        }
        Ok(result) => {
            let source = result?;
            paid?;
            common.consume_steps(0).map_err(formal)?;
            Ok(PaidNormalRuntimeProjectionSourcesV1 {
                source,
                core,
                memory,
                _fee: fee,
            })
        }
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn capture_current_normal_projection_runtime_sources_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &BoundNormalTickParentV1<'_>,
    read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, V, E, C>,
    work: &mut RuntimeProjectionAuxWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PaidNormalRuntimeProjectionSourcesV1, BodyMutationError> {
    capture(parent, read, ctx, work, common, live, Scope::Current)
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn capture_qualified_normal_projection_runtime_sources_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &BoundNormalTickParentV1<'_>,
    read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, V, E, C>,
    work: &mut RuntimeProjectionAuxWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PaidNormalRuntimeProjectionSourcesV1, BodyMutationError> {
    capture(parent, read, ctx, work, common, live, Scope::Qualified)
}
