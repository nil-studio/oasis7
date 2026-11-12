//! State-only reborrow of an already-started step, with a sole Common child.
//! This module is nested under incremental_step so it can preserve the actual
//! clock without exposing a raw step constructor or cloning its event counter.
use super::*;
use crate::world::{
    incremental_body_event::materials::CorePolicyMaterialContextV1,
    incremental_normal_action_factory_schedule::FactoryScheduleWorkV1,
};
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1;
use oasis7_formal_protocol::incremental_value::{ValueReadContext, ValueTreeError};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_read::WorldFieldReadContextV1,
};

fn invalid() -> BodyMutationError {
    BodyMutationError::Invalid("factory schedule state IO accounting")
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn paid<M, P, V, W, D, E, C, PE, PC, CW, ME, T>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    work: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
    run: impl FnOnce(
        &mut IwsStepContext<'_, '_, '_, '_, M, V, W, &mut dyn FnMut() -> bool>,
        &mut WorldFieldReadContextV1<'_, P, V, &mut E, &mut dyn FnMut() -> bool>,
        &mut WorldFieldReadContextV1<'_, P, V, &mut PE, &mut dyn FnMut() -> bool>,
        &mut MemberRecordWriteContext<'_, W, &mut dyn FnMut() -> bool>,
        &mut CorePolicyMaterialContextV1<'_, M, &mut ME>,
        &mut FactoryScheduleWorkV1,
    ) -> Result<T, BodyMutationError>,
) -> Result<T, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader
        + ValueNodeWriter
        + oasis7_formal_protocol::incremental_state::StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
{
    let now = step.now;
    step.transaction.contribute(|tx| {
        tx.validate_guard(&tx.guard(),live)?;
        common.consume_steps(1).map_err(|e|BodyMutationError::Original(Box::new(e)))?;
        if !tx.uses_budget(common.memory())||!step.mutation.memory.same_pool(common.memory())
            ||!read.values.memory.same_pool(common.memory())||!parent.values.memory.same_pool(common.memory())
            ||!write.memory.same_pool(common.memory())||!materials.parent.uses_budget(common.memory())
            ||read.values.class!=MemoryClass::Ordinary||parent.values.class!=MemoryClass::Ordinary {
            return Err(invalid());
        }
        // Fixed RootSet/copy/digest validation paths in the concrete state
        // writers. Traversal, key encoding, copies and codec work have their
        // actual lanes lowered below; this is not a whole-writer reservation.
        let metadata=materials.roots.world_id.len().checked_add(6*oasis7_formal_protocol::incremental_state::STATE_ROOT_BYTES)
            .and_then(|n|n.checked_add(1024)).and_then(|n|n.checked_mul(128)).ok_or_else(invalid)?;
        common.charge(u64::try_from(metadata).map_err(|_|invalid())?,u64::try_from(metadata).map_err(|_|invalid())?).map_err(|e|BodyMutationError::Original(Box::new(e)))?;
        let (_,meter,stop)=common.parts();let child=meter.begin_child().map_err(|e|BodyMutationError::Original(Box::new(e)))?;
        let visits=child.remaining_steps()/58;let bytes=child.remaining_read_bytes()/36;
        macro_rules! steps {
            ($m:ident)=> { $m!(
                read.values.io.nodes_remaining,
                parent.values.io.nodes_remaining,
                work.lifecycle.steps_remaining,
                work.lifecycle.factory.cells_remaining,
                work.lifecycle.factory.scan.nodes_remaining,
                work.lifecycle.factory.scan.leaves_remaining,
                work.lifecycle.industry.cells_remaining,
                work.lifecycle.industry.scan.nodes_remaining,
                work.lifecycle.industry.scan.leaves_remaining,
                work.lifecycle.lifecycle.cells_remaining,
                work.lifecycle.native_scan.nodes_remaining,
                work.lifecycle.native_scan.leaves_remaining,
                work.lifecycle.native_read.json.cells_remaining,
                work.lifecycle.native_read.json.json_nodes_remaining,
                work.builds.cells_remaining,
                work.receipts.cells_remaining,
                work.authority.cells_remaining,
                work.authority_cells,
                work.authority_scan.nodes_remaining,
                work.authority_scan.leaves_remaining,
                work.factory.cells_remaining,
                work.logistics.cells_remaining,
                work.inventory_directory.visits_remaining,
                work.inventory_family.records_remaining,
                work.inventory_family.index_steps_remaining,
                work.material_profiles.scan.nodes_remaining,
                work.material_profiles.scan.leaves_remaining,
                work.material_profiles.cells_remaining,
                work.invocation.instance_scan.nodes_remaining,
                work.invocation.instance_scan.leaves_remaining,
                work.invocation.registry_scan.nodes_remaining,
                work.invocation.registry_scan.leaves_remaining,
                work.config.cells_remaining,
                work.config.json.cells_remaining,
                work.config.json.json_nodes_remaining,
                work.config_scan.nodes_remaining,
                work.config_scan.leaves_remaining,
                step.mutation.io.nodes_remaining,
                step.original_work.visits_remaining,
                write.io.nodes_remaining,
                materials.read_io.nodes_remaining,
                work.lifecycle.native_write.cells_remaining,
                work.lifecycle.native_write.json.cells_remaining,
                work.lifecycle.native_write.json.json_nodes_remaining,
                work.lifecycle.records_remaining,
                materials.family_work.records_remaining,
                materials.family_work.index_steps_remaining,
                materials.group_work.steps_remaining,
                *materials.normalization_cells_remaining,
                work.logistics_write.cells_remaining,
                work.logistics_write.scan.nodes_remaining,
                work.logistics_write.scan.leaves_remaining
            ) };
        }
        macro_rules! bytes {
            ($m:ident)=> { $m!(
                read.values.io.read_bytes_remaining,
                read.values.io.write_bytes_remaining,
                parent.values.io.read_bytes_remaining,
                parent.values.io.write_bytes_remaining,
                read.key_work.bytes_remaining,
                parent.key_work.bytes_remaining,
                work.lifecycle.factory.scan.read_bytes_remaining,
                work.lifecycle.industry.scan.read_bytes_remaining,
                work.lifecycle.native_scan.read_bytes_remaining,
                work.authority_scan.read_bytes_remaining,
                work.lifecycle.bytes_remaining,
                work.lifecycle.lifecycle.comparison_bytes_remaining,
                work.builds.comparison_bytes_remaining,
                work.receipts.bytes_remaining,
                work.authority.bytes_remaining,
                work.factory.bytes_remaining,
                work.logistics.bytes_remaining,
                work.inventory_directory.comparison_bytes_remaining,
                work.material_profiles.scan.read_bytes_remaining,
                work.invocation.instance_scan.read_bytes_remaining,
                work.invocation.registry_scan.read_bytes_remaining,
                work.invocation.comparison_bytes_remaining,
                work.config_scan.read_bytes_remaining,
                step.mutation.io.read_bytes_remaining,
                step.mutation.io.write_bytes_remaining,
                step.original_work.bytes_remaining,
                write.io.read_bytes_remaining,
                write.io.write_bytes_remaining,
                write.key_work.bytes_remaining,
                materials.read_io.read_bytes_remaining,
                materials.read_io.write_bytes_remaining,
                materials.read_keys.bytes_remaining,
                materials.write_keys.bytes_remaining,
                materials.scope_work.comparison_bytes_remaining,
                *materials.normalization_comparison_bytes_remaining,
                work.logistics_write.scan.read_bytes_remaining
            ) };
        }
        macro_rules! entries {
            ($m:ident)=> { $m!(
                work.lifecycle.pending_entries_remaining,
                work.lifecycle.lifecycle.pending_entries_remaining,
                work.inventory_directory.pending_entries_remaining,
                work.invocation.pending_entries,
                work.lifecycle.output_group.pending_entries_remaining,
                materials.scope_work.pending_entries_remaining
            ) };
        }
        macro_rules! clamp_steps {($($lane:expr),*)=>{$(($lane)=($lane).min(visits);)*};}
        macro_rules! clamp_bytes {($($lane:expr),*)=>{$(($lane)=($lane).min(bytes);)*};}
        macro_rules! clamp_entries {($($lane:expr),*)=>{$(($lane)=($lane).min(usize::try_from(visits).unwrap_or(usize::MAX));)*};}
        macro_rules! snapshot {($($lane:expr),*)=>{[$($lane),*]};}
        macro_rules! entry_snapshot {($($lane:expr),*)=>{[$(u64::try_from($lane).map_err(|_|invalid())?),*]};}
        steps!(clamp_steps);bytes!(clamp_bytes);entries!(clamp_entries);
        let before_steps=steps!(snapshot);let before_bytes=bytes!(snapshot);let before_entries=entries!(entry_snapshot);
        let facts_before=tx.ordered_fact_count();
        let result=(|| {
            let stop=std::cell::RefCell::new(stop);
            let mut step_stop=|| (step.mutation.cancelled)()||(stop.borrow_mut())();
            let mut read_stop=|| (read.cancelled)()||(stop.borrow_mut())();
            let mut parent_stop=|| (parent.cancelled)()||(stop.borrow_mut())();
            let mut write_stop=|| (write.cancelled)()||(stop.borrow_mut())();
            let mut mutation=BodyMutationContext {sink:&mut *step.mutation.sink,memory:step.mutation.memory,
                io:&mut *step.mutation.io,cancelled:&mut step_stop as &mut dyn FnMut()->bool};
            // This checks the actual time. Step::begin would ADVANCE it again.
            crate::world::incremental_clock::check_clock(now,tx,&mut mutation)?;
            let mut scoped_step=IwsStepContext {transaction:tx,values:step.values,mutation:&mut mutation,
                original_work:&mut *step.original_work,now,world_events:None};
            let mut scoped_read=WorldFieldReadContextV1 {root:read.root,state:read.state,
                values:ValueReadContext {source:read.values.source,memory:read.values.memory,class:read.values.class,io:&mut *read.values.io},
                key_work:&mut *read.key_work,semantic_bytes:read.semantic_bytes,record_bytes:read.record_bytes,
                take_state_error:&mut read.take_state_error,cancelled:&mut read_stop as &mut dyn FnMut()->bool};
            let mut scoped_parent=WorldFieldReadContextV1 {root:parent.root,state:parent.state,
                values:ValueReadContext {source:parent.values.source,memory:parent.values.memory,class:parent.values.class,io:&mut *parent.values.io},
                key_work:&mut *parent.key_work,semantic_bytes:parent.semantic_bytes,record_bytes:parent.record_bytes,
                take_state_error:&mut parent.take_state_error,cancelled:&mut parent_stop as &mut dyn FnMut()->bool};
            let mut scoped_write=MemberRecordWriteContext {sink:&mut *write.sink,memory:write.memory,
                io:&mut *write.io,key_work:&mut *write.key_work,limits:write.limits,cancelled:&mut write_stop as &mut dyn FnMut()->bool};
            let mut scoped_materials=CorePolicyMaterialContextV1 {parent:materials.parent,roots:materials.roots,pages:materials.pages,
                read_io:&mut *materials.read_io,read_keys:&mut *materials.read_keys,write_keys:&mut *materials.write_keys,
                semantic_bytes:materials.semantic_bytes,record_bytes:materials.record_bytes,write_limits:materials.write_limits,
                take_state_error:&mut materials.take_state_error,family_work:&mut *materials.family_work,
                scope_work:&mut *materials.scope_work,group_work:&mut *materials.group_work,
                normalization_max_union_entries:materials.normalization_max_union_entries,
                normalization_cells_remaining:&mut *materials.normalization_cells_remaining,
                normalization_comparison_bytes_remaining:&mut *materials.normalization_comparison_bytes_remaining};
            run(&mut scoped_step,&mut scoped_read,&mut scoped_parent,&mut scoped_write,&mut scoped_materials,work)
        })();
        let after_steps=steps!(snapshot);let after_bytes=bytes!(snapshot);let after_entries=entries!(entry_snapshot);
        let debit=|before:&[u64],after:&[u64]|before.iter().zip(after).try_fold(0u64,|n,(b,a)|n.checked_add(b.checked_sub(*a)?));
        let counted_steps=debit(&before_steps,&after_steps).and_then(|n|n.checked_add(debit(&before_entries,&after_entries)?));
        let Some(steps)=counted_steps else{return result.and(Err(invalid()));};
        let Some(bytes)=debit(&before_bytes,&after_bytes) else{return result.and(Err(invalid()));};
        let paid=child.consume(steps,bytes).map_err(|e|BodyMutationError::Original(Box::new(e)));child.finish();
        let value=result?;paid?;
        if tx.ordered_fact_count()!=facts_before {return Err(BodyMutationError::Invalid("factory schedule state bridge emitted a fact"));}
        common.consume_steps(0).map_err(|e|BodyMutationError::Original(Box::new(e)))?;
        if (step.mutation.cancelled)()||(read.cancelled)()||(parent.cancelled)()||(write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.validate_guard(&tx.guard(),live)?;Ok(value)
    })
}
