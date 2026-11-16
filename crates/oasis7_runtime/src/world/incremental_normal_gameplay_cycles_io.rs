//! One actual Common child for the gameplay state's three original IO lanes.
//! Source counters are clamped in place and settled even when a reducer fails.
use super::*;
use crate::world::{
    incremental_body_event::materials::CorePolicyMaterialContextV1,
    incremental_normal_tail::NormalTailWorkV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
};
use std::cell::RefCell;
fn bad() -> GameplayCycleErrorV1 {
    ValueTreeError::ResourceLimited("gameplay continuous meter").into()
}
fn remaining(w: &NormalGameplayCycleWorkV1) -> ([u64; 20], [u64; 6]) {
    (
        [
            w.directory.visits_remaining,
            w.directory.pending_entries_remaining as u64,
            w.selected_scan.nodes_remaining,
            w.selected_scan.leaves_remaining,
            w.cells_remaining,
            w.pending_entries_remaining as u64,
            w.native.json.cells_remaining,
            w.native.json.json_nodes_remaining,
            w.native_scan.nodes_remaining,
            w.native_scan.leaves_remaining,
            w.native_write.cells_remaining,
            w.native_write.json.cells_remaining,
            w.native_write.json.json_nodes_remaining,
            w.due_scan.nodes_remaining,
            w.due_scan.buckets_remaining,
            w.due_scan.instances_remaining,
            w.meta.steps_remaining,
            w.diplomacy_index.cells_remaining,
            w.diplomacy_index.pending_entries_remaining as u64,
            0,
        ],
        [
            w.directory.comparison_bytes_remaining,
            w.selected_scan.read_bytes_remaining,
            w.native_scan.read_bytes_remaining,
            w.due_scan.read_bytes_remaining,
            w.meta.comparison_bytes_remaining,
            w.diplomacy_index.comparison_bytes_remaining,
        ],
    )
}
fn clamp(w: &mut NormalGameplayCycleWorkV1, s: u64, b: u64) {
    for n in [
        &mut w.directory.visits_remaining,
        &mut w.selected_scan.nodes_remaining,
        &mut w.selected_scan.leaves_remaining,
        &mut w.cells_remaining,
        &mut w.native.json.cells_remaining,
        &mut w.native.json.json_nodes_remaining,
        &mut w.native_scan.nodes_remaining,
        &mut w.native_scan.leaves_remaining,
        &mut w.native_write.cells_remaining,
        &mut w.native_write.json.cells_remaining,
        &mut w.native_write.json.json_nodes_remaining,
        &mut w.due_scan.nodes_remaining,
        &mut w.due_scan.buckets_remaining,
        &mut w.due_scan.instances_remaining,
        &mut w.meta.steps_remaining,
        &mut w.diplomacy_index.cells_remaining,
    ] {
        *n = (*n).min(s);
    }
    for n in [
        &mut w.directory.comparison_bytes_remaining,
        &mut w.selected_scan.read_bytes_remaining,
        &mut w.native_scan.read_bytes_remaining,
        &mut w.due_scan.read_bytes_remaining,
        &mut w.meta.comparison_bytes_remaining,
        &mut w.diplomacy_index.comparison_bytes_remaining,
    ] {
        *n = (*n).min(b);
    }
    let s = usize::try_from(s).unwrap_or(usize::MAX);
    w.directory.pending_entries_remaining = w.directory.pending_entries_remaining.min(s);
    w.pending_entries_remaining = w.pending_entries_remaining.min(s);
    w.diplomacy_index.pending_entries_remaining =
        w.diplomacy_index.pending_entries_remaining.min(s);
}
fn material_remaining<M, E>(m: &CorePolicyMaterialContextV1<'_, M, E>) -> ([u64; 6], [u64; 6]) {
    (
        [
            m.read_io.nodes_remaining,
            m.family_work.records_remaining,
            m.family_work.index_steps_remaining,
            m.scope_work.pending_entries_remaining as u64,
            m.group_work.steps_remaining,
            *m.normalization_cells_remaining,
        ],
        [
            m.read_io.read_bytes_remaining,
            m.read_io.write_bytes_remaining,
            m.read_keys.bytes_remaining,
            m.write_keys.bytes_remaining,
            m.scope_work.comparison_bytes_remaining,
            *m.normalization_comparison_bytes_remaining,
        ],
    )
}
fn material_clamp<M, E>(m: &mut CorePolicyMaterialContextV1<'_, M, E>, s: u64, b: u64) {
    for n in [
        &mut m.read_io.nodes_remaining,
        &mut m.family_work.records_remaining,
        &mut m.family_work.index_steps_remaining,
        &mut m.group_work.steps_remaining,
        &mut *m.normalization_cells_remaining,
    ] {
        *n = (*n).min(s);
    }
    m.scope_work.pending_entries_remaining = m
        .scope_work
        .pending_entries_remaining
        .min(usize::try_from(s).unwrap_or(usize::MAX));
    for n in [
        &mut m.read_io.read_bytes_remaining,
        &mut m.read_io.write_bytes_remaining,
        &mut m.read_keys.bytes_remaining,
        &mut m.write_keys.bytes_remaining,
        &mut m.scope_work.comparison_bytes_remaining,
        &mut *m.normalization_comparison_bytes_remaining,
    ] {
        *n = (*n).min(b);
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn run<M, P, V, R, W, E, C, PE, PC, CW, ME, T>(
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    work: &mut NormalGameplayCycleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    run: impl FnOnce(
        &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, &mut E, &mut dyn FnMut() -> bool>,
        &mut WorldFieldReadContextV1<'_, P, V, &mut PE, &mut dyn FnMut() -> bool>,
        &mut MemberRecordWriteContext<'_, W, &mut dyn FnMut() -> bool>,
        &mut CorePolicyMaterialContextV1<'_, M, ME>,
        &mut NormalGameplayCycleWorkV1,
        &mut NormalTailWorkV1<'_, '_>,
    ) -> Result<T, GameplayCycleErrorV1>,
) -> Result<T, GameplayCycleErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
{
    if ctx.values.class != MemoryClass::Ordinary
        || parent.values.class != MemoryClass::Ordinary
        || !ctx.values.memory.same_pool(common.memory())
        || !parent.values.memory.same_pool(common.memory())
        || !write.memory.same_pool(common.memory())
        || !ctx.transaction.uses_budget(common.memory())
    {
        return Err(bad());
    }
    common
        .consume_steps(1)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let (memory, meter, stop) = common.parts();
    let child = meter
        .begin_child()
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    // There are 25 actual I/O/typed step lanes and 13 byte lanes. Leave capacity
    // for explicit actual-shape copy/hash charges in this same child.
    let s = child.remaining_steps() / 64;
    let b = child.remaining_read_bytes() / 32;
    for io in [&mut *ctx.values.io, &mut *parent.values.io, &mut *write.io] {
        io.nodes_remaining = io.nodes_remaining.min(s);
        io.read_bytes_remaining = io.read_bytes_remaining.min(b);
        io.write_bytes_remaining = io.write_bytes_remaining.min(b);
    }
    for key in [
        &mut ctx.key_work.bytes_remaining,
        &mut parent.key_work.bytes_remaining,
        &mut write.key_work.bytes_remaining,
    ] {
        *key = (*key).min(b);
    }
    clamp(work, s, b);
    material_clamp(materials, s, b);
    let before_io = (
        [
            ctx.values.io.nodes_remaining,
            parent.values.io.nodes_remaining,
            write.io.nodes_remaining,
        ],
        [
            ctx.values.io.read_bytes_remaining,
            ctx.values.io.write_bytes_remaining,
            parent.values.io.read_bytes_remaining,
            parent.values.io.write_bytes_remaining,
            write.io.read_bytes_remaining,
            write.io.write_bytes_remaining,
            ctx.key_work.bytes_remaining,
            parent.key_work.bytes_remaining,
            write.key_work.bytes_remaining,
        ],
    );
    let before_work = remaining(work);
    let before_material = material_remaining(materials);
    let result = {
        let stop = RefCell::new(stop);
        let mut cancel = || (*stop.borrow_mut())() || (ctx.cancelled)();
        let mut parent_cancel = || (*stop.borrow_mut())() || (parent.cancelled)();
        let mut writer_cancel = || (*stop.borrow_mut())() || (write.cancelled)();
        let mut tail_cancel = || (*stop.borrow_mut())();
        let mut tail = NormalTailWorkV1::from_existing_child(memory, &child, &mut tail_cancel);
        let mut current = WorldFieldOverlayContextV1 {
            transaction: &mut *ctx.transaction,
            values: ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut cancel as &mut dyn FnMut() -> bool,
        };
        let mut source = WorldFieldReadContextV1 {
            root: parent.root,
            state: parent.state,
            values: ValueReadContext {
                source: parent.values.source,
                memory: parent.values.memory,
                class: parent.values.class,
                io: &mut *parent.values.io,
            },
            key_work: &mut *parent.key_work,
            semantic_bytes: parent.semantic_bytes,
            record_bytes: parent.record_bytes,
            take_state_error: &mut parent.take_state_error,
            cancelled: &mut parent_cancel as &mut dyn FnMut() -> bool,
        };
        let mut output = MemberRecordWriteContext {
            sink: &mut *write.sink,
            memory: write.memory,
            io: &mut *write.io,
            key_work: &mut *write.key_work,
            limits: write.limits,
            cancelled: &mut writer_cancel as &mut dyn FnMut() -> bool,
        };
        run(
            &mut current,
            &mut source,
            &mut output,
            materials,
            work,
            &mut tail,
        )
    };
    let after_io = (
        [
            ctx.values.io.nodes_remaining,
            parent.values.io.nodes_remaining,
            write.io.nodes_remaining,
        ],
        [
            ctx.values.io.read_bytes_remaining,
            ctx.values.io.write_bytes_remaining,
            parent.values.io.read_bytes_remaining,
            parent.values.io.write_bytes_remaining,
            write.io.read_bytes_remaining,
            write.io.write_bytes_remaining,
            ctx.key_work.bytes_remaining,
            parent.key_work.bytes_remaining,
            write.key_work.bytes_remaining,
        ],
    );
    let after_work = remaining(work);
    let after_material = material_remaining(materials);
    let accounting = (|| {
        let mut steps = 0u64;
        let mut bytes = 0u64;
        for (a, z) in before_io
            .0
            .into_iter()
            .zip(after_io.0)
            .chain(before_work.0.into_iter().zip(after_work.0))
            .chain(before_material.0.into_iter().zip(after_material.0))
        {
            steps = steps
                .checked_add(a.checked_sub(z).ok_or_else(bad)?)
                .ok_or_else(bad)?;
        }
        for (a, z) in before_io
            .1
            .into_iter()
            .zip(after_io.1)
            .chain(before_work.1.into_iter().zip(after_work.1))
            .chain(before_material.1.into_iter().zip(after_material.1))
        {
            bytes = bytes
                .checked_add(a.checked_sub(z).ok_or_else(bad)?)
                .ok_or_else(bad)?;
        }
        child
            .consume(steps, bytes)
            .map_err(|e| GameplayCycleErrorV1::from(BodyMutationError::Original(Box::new(e))))
    })();
    if accounting.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            ctx.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    child.finish();
    match (result, accounting) {
        (Err(original), _) => Err(original),
        (Ok(_), Err(error)) => Err(error),
        (Ok(value), Ok(())) => Ok(value),
    }
}
