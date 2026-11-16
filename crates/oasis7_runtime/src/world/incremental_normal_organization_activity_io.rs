//! One actual Common child for the gameplay state's three original IO lanes.
//! Source counters are clamped in place and settled even when a reducer fails.
use super::*;
use oasis7_formal_protocol::incremental_value::ValueReadContext;
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
fn bad() -> BodyMutationError {
    ValueTreeError::ResourceLimited("organization continuous meter").into()
}
fn remaining(w:&NormalOrganizationTailWorkV1)->([u64;20],[u64;6]) { ([w.scan.nodes_remaining,w.scan.leaves_remaining,w.read.json.cells_remaining,w.read.json.json_nodes_remaining,w.write.cells_remaining,w.write.json.cells_remaining,w.write.json.json_nodes_remaining,w.eligibility.read.cells_remaining,w.eligibility.typed.json.cells_remaining,w.eligibility.typed.json.json_nodes_remaining,w.eligibility.scan.nodes_remaining,w.eligibility.scan.leaves_remaining,w.eligibility.revocation.json.cells_remaining,w.eligibility.revocation.json.json_nodes_remaining,w.pending_entries_remaining as u64,w.eligibility.pending_entries_remaining as u64,w.cells_remaining,w.diplomacy_index.cells_remaining,w.diplomacy_index.pending_entries_remaining as u64,w.orders.pending_entries_remaining as u64],[w.scan.read_bytes_remaining,w.eligibility.read.comparison_bytes_remaining,w.eligibility.scan.read_bytes_remaining,w.diplomacy_index.comparison_bytes_remaining,w.orders.ids.comparison_bytes_remaining,w.orders.merge_comparison_bytes_remaining]) }
fn clamp(w:&mut NormalOrganizationTailWorkV1,s:u64,b:u64){
w.scan.nodes_remaining=w.scan.nodes_remaining.min(s);
w.scan.leaves_remaining=w.scan.leaves_remaining.min(s);
w.read.json.cells_remaining=w.read.json.cells_remaining.min(s);
w.read.json.json_nodes_remaining=w.read.json.json_nodes_remaining.min(s);
w.write.cells_remaining=w.write.cells_remaining.min(s);
w.write.json.cells_remaining=w.write.json.cells_remaining.min(s);
w.write.json.json_nodes_remaining=w.write.json.json_nodes_remaining.min(s);
w.eligibility.read.cells_remaining=w.eligibility.read.cells_remaining.min(s);
w.eligibility.typed.json.cells_remaining=w.eligibility.typed.json.cells_remaining.min(s);
w.eligibility.typed.json.json_nodes_remaining=w.eligibility.typed.json.json_nodes_remaining.min(s);
w.eligibility.scan.nodes_remaining=w.eligibility.scan.nodes_remaining.min(s);
w.eligibility.scan.leaves_remaining=w.eligibility.scan.leaves_remaining.min(s);
w.eligibility.revocation.json.cells_remaining=w.eligibility.revocation.json.cells_remaining.min(s);
w.eligibility.revocation.json.json_nodes_remaining=w.eligibility.revocation.json.json_nodes_remaining.min(s);
w.scan.read_bytes_remaining=w.scan.read_bytes_remaining.min(b);
w.eligibility.read.comparison_bytes_remaining=w.eligibility.read.comparison_bytes_remaining.min(b);
w.eligibility.scan.read_bytes_remaining=w.eligibility.scan.read_bytes_remaining.min(b);
w.pending_entries_remaining=w.pending_entries_remaining.min(usize::try_from(s).unwrap_or(usize::MAX));
w.eligibility.pending_entries_remaining=w.eligibility.pending_entries_remaining.min(usize::try_from(s).unwrap_or(usize::MAX));
w.cells_remaining=w.cells_remaining.min(s);
w.diplomacy_index.cells_remaining=w.diplomacy_index.cells_remaining.min(s);
w.diplomacy_index.pending_entries_remaining=w.diplomacy_index.pending_entries_remaining.min(usize::try_from(s).unwrap_or(usize::MAX));
w.orders.pending_entries_remaining=w.orders.pending_entries_remaining.min(usize::try_from(s).unwrap_or(usize::MAX));
w.diplomacy_index.comparison_bytes_remaining=w.diplomacy_index.comparison_bytes_remaining.min(b);
w.orders.ids.comparison_bytes_remaining=w.orders.ids.comparison_bytes_remaining.min(b);
w.orders.merge_comparison_bytes_remaining=w.orders.merge_comparison_bytes_remaining.min(b);
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
    work: &mut NormalOrganizationTailWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    run: impl FnOnce(
        &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, &mut E, &mut dyn FnMut() -> bool>,
        &mut WorldFieldReadContextV1<'_, P, V, &mut PE, &mut dyn FnMut() -> bool>,
        &mut MemberRecordWriteContext<'_, W, &mut dyn FnMut() -> bool>,
        &mut CorePolicyMaterialContextV1<'_, M, ME>,
        &mut NormalOrganizationTailWorkV1,
        &mut NormalTailWorkV1<'_, '_>,
    ) -> Result<T, BodyMutationError>,
) -> Result<T, BodyMutationError>
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
    let original_work=remaining(work);
    let original_material=material_remaining(materials);
    let original_io=(
      [ctx.values.io.nodes_remaining,parent.values.io.nodes_remaining,write.io.nodes_remaining],
      [ctx.values.io.read_bytes_remaining,ctx.values.io.write_bytes_remaining,parent.values.io.read_bytes_remaining,parent.values.io.write_bytes_remaining,write.io.read_bytes_remaining,write.io.write_bytes_remaining,ctx.key_work.bytes_remaining,parent.key_work.bytes_remaining,write.key_work.bytes_remaining]);
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
    let execution = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
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
    }));
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
    // Return unspent temporary quotas to each original source lane.
    work.scan.nodes_remaining=original_work.0[0].saturating_sub(before_work.0[0].saturating_sub(after_work.0[0]));
    work.scan.leaves_remaining=original_work.0[1].saturating_sub(before_work.0[1].saturating_sub(after_work.0[1]));
    work.read.json.cells_remaining=original_work.0[2].saturating_sub(before_work.0[2].saturating_sub(after_work.0[2]));
    work.read.json.json_nodes_remaining=original_work.0[3].saturating_sub(before_work.0[3].saturating_sub(after_work.0[3]));
    work.write.cells_remaining=original_work.0[4].saturating_sub(before_work.0[4].saturating_sub(after_work.0[4]));
    work.write.json.cells_remaining=original_work.0[5].saturating_sub(before_work.0[5].saturating_sub(after_work.0[5]));
    work.write.json.json_nodes_remaining=original_work.0[6].saturating_sub(before_work.0[6].saturating_sub(after_work.0[6]));
    work.eligibility.read.cells_remaining=original_work.0[7].saturating_sub(before_work.0[7].saturating_sub(after_work.0[7]));
    work.eligibility.typed.json.cells_remaining=original_work.0[8].saturating_sub(before_work.0[8].saturating_sub(after_work.0[8]));
    work.eligibility.typed.json.json_nodes_remaining=original_work.0[9].saturating_sub(before_work.0[9].saturating_sub(after_work.0[9]));
    work.eligibility.scan.nodes_remaining=original_work.0[10].saturating_sub(before_work.0[10].saturating_sub(after_work.0[10]));
    work.eligibility.scan.leaves_remaining=original_work.0[11].saturating_sub(before_work.0[11].saturating_sub(after_work.0[11]));
    work.eligibility.revocation.json.cells_remaining=original_work.0[12].saturating_sub(before_work.0[12].saturating_sub(after_work.0[12]));
    work.eligibility.revocation.json.json_nodes_remaining=original_work.0[13].saturating_sub(before_work.0[13].saturating_sub(after_work.0[13]));
    work.pending_entries_remaining=usize::try_from(original_work.0[14].saturating_sub(before_work.0[14].saturating_sub(after_work.0[14]))).unwrap_or(usize::MAX);
    work.eligibility.pending_entries_remaining=usize::try_from(original_work.0[15].saturating_sub(before_work.0[15].saturating_sub(after_work.0[15]))).unwrap_or(usize::MAX);
    work.scan.read_bytes_remaining=original_work.1[0].saturating_sub(before_work.1[0].saturating_sub(after_work.1[0]));
    work.eligibility.read.comparison_bytes_remaining=original_work.1[1].saturating_sub(before_work.1[1].saturating_sub(after_work.1[1]));
    work.eligibility.scan.read_bytes_remaining=original_work.1[2].saturating_sub(before_work.1[2].saturating_sub(after_work.1[2]));
    ctx.values.io.nodes_remaining=original_io.0[0].saturating_sub(before_io.0[0].saturating_sub(after_io.0[0]));
    parent.values.io.nodes_remaining=original_io.0[1].saturating_sub(before_io.0[1].saturating_sub(after_io.0[1]));
    write.io.nodes_remaining=original_io.0[2].saturating_sub(before_io.0[2].saturating_sub(after_io.0[2]));
    ctx.values.io.read_bytes_remaining=original_io.1[0].saturating_sub(before_io.1[0].saturating_sub(after_io.1[0]));
    ctx.values.io.write_bytes_remaining=original_io.1[1].saturating_sub(before_io.1[1].saturating_sub(after_io.1[1]));
    parent.values.io.read_bytes_remaining=original_io.1[2].saturating_sub(before_io.1[2].saturating_sub(after_io.1[2]));
    parent.values.io.write_bytes_remaining=original_io.1[3].saturating_sub(before_io.1[3].saturating_sub(after_io.1[3]));
    write.io.read_bytes_remaining=original_io.1[4].saturating_sub(before_io.1[4].saturating_sub(after_io.1[4]));
    write.io.write_bytes_remaining=original_io.1[5].saturating_sub(before_io.1[5].saturating_sub(after_io.1[5]));
    ctx.key_work.bytes_remaining=original_io.1[6].saturating_sub(before_io.1[6].saturating_sub(after_io.1[6]));
    parent.key_work.bytes_remaining=original_io.1[7].saturating_sub(before_io.1[7].saturating_sub(after_io.1[7]));
    write.key_work.bytes_remaining=original_io.1[8].saturating_sub(before_io.1[8].saturating_sub(after_io.1[8]));
    materials.read_io.nodes_remaining=original_material.0[0].saturating_sub(before_material.0[0].saturating_sub(after_material.0[0]));
    materials.family_work.records_remaining=original_material.0[1].saturating_sub(before_material.0[1].saturating_sub(after_material.0[1]));
    materials.family_work.index_steps_remaining=original_material.0[2].saturating_sub(before_material.0[2].saturating_sub(after_material.0[2]));
    materials.scope_work.pending_entries_remaining=usize::try_from(original_material.0[3].saturating_sub(before_material.0[3].saturating_sub(after_material.0[3]))).unwrap_or(usize::MAX);
    materials.group_work.steps_remaining=original_material.0[4].saturating_sub(before_material.0[4].saturating_sub(after_material.0[4]));
    *materials.normalization_cells_remaining=original_material.0[5].saturating_sub(before_material.0[5].saturating_sub(after_material.0[5]));
    materials.read_io.read_bytes_remaining=original_material.1[0].saturating_sub(before_material.1[0].saturating_sub(after_material.1[0]));
    materials.read_io.write_bytes_remaining=original_material.1[1].saturating_sub(before_material.1[1].saturating_sub(after_material.1[1]));
    materials.read_keys.bytes_remaining=original_material.1[2].saturating_sub(before_material.1[2].saturating_sub(after_material.1[2]));
    materials.write_keys.bytes_remaining=original_material.1[3].saturating_sub(before_material.1[3].saturating_sub(after_material.1[3]));
    materials.scope_work.comparison_bytes_remaining=original_material.1[4].saturating_sub(before_material.1[4].saturating_sub(after_material.1[4]));
    *materials.normalization_comparison_bytes_remaining=original_material.1[5].saturating_sub(before_material.1[5].saturating_sub(after_material.1[5]));
    work.cells_remaining=original_work.0[16].saturating_sub(before_work.0[16].saturating_sub(after_work.0[16]));
    work.diplomacy_index.cells_remaining=original_work.0[17].saturating_sub(before_work.0[17].saturating_sub(after_work.0[17]));
    work.diplomacy_index.pending_entries_remaining=usize::try_from(original_work.0[18].saturating_sub(before_work.0[18].saturating_sub(after_work.0[18]))).unwrap_or(usize::MAX);
    work.orders.pending_entries_remaining=usize::try_from(original_work.0[19].saturating_sub(before_work.0[19].saturating_sub(after_work.0[19]))).unwrap_or(usize::MAX);
    work.diplomacy_index.comparison_bytes_remaining=original_work.1[3].saturating_sub(before_work.1[3].saturating_sub(after_work.1[3]));
    work.orders.ids.comparison_bytes_remaining=original_work.1[4].saturating_sub(before_work.1[4].saturating_sub(after_work.1[4]));
    work.orders.merge_comparison_bytes_remaining=original_work.1[5].saturating_sub(before_work.1[5].saturating_sub(after_work.1[5]));
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
            .map_err(|e| BodyMutationError::from(BodyMutationError::Original(Box::new(e))))
    })();
    if accounting.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            ctx.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    child.finish();
    let result=match execution { Ok(result)=>result,Err(payload)=>{
        let _:Result<(),oasis7_formal_protocol::incremental_execution::ExecutionError>=ctx.transaction.contribute(|_|Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted));
        std::panic::resume_unwind(payload)
    }};
    match (result, accounting) {
        (Err(original), _) => Err(original),
        (Ok(_), Err(error)) => Err(error),
        (Ok(value), Ok(())) => Ok(value),
    }
}
