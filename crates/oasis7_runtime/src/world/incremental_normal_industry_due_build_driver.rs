//! The build half of the single industrial due phase. This helper does not
//! produce whole-phase/tick completion. Recipe selection occurs only after
//! every actual build event has been counted; routing retains the original
//! deferred economy flush points.
use super::*;
use crate::world::{
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_module_invocation::{
        AcceptedInvocationDirectoryV1, CurrentInvocationEnumerationWorkV1,
        RootModuleExecutionContextV1, RootModuleExecutionErrorV1,
    },
    incremental_normal_action_factory_schedule::{FactoryScheduleSourcesV1, FactoryScheduleWorkV1},
    incremental_normal_tail::NormalTailWorkV1,
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    incremental_material::OriginalObjectStreamReader,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_world_state::incremental_field_read::WorldFieldReadContextV1;

impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_due_builds_v1<P, CE, CC, PE, PC, ME>(
        &mut self,
        deferred: &mut super::deferred::DueEventQueueV1,
        materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
        current: &mut WorldFieldReadContextV1<'_, P, R, CE, CC>,
        parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        sources: &FactoryScheduleSourcesV1<'_>,
        work: &mut FactoryScheduleWorkV1,
        normal: &mut NormalTailWorkV1<'_, '_>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<(), RootModuleExecutionErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        CE: FnMut() -> Option<ValueTreeError>,
        CC: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
        ME: FnMut() -> Option<ValueTreeError>,
    {
        let result = (|| {
            let live = live_owner();
            let guard = self.ctx.transaction.guard();
            let order = {
                let mut step = IwsStepContext::borrow_counted_at(
                    self.ctx.transaction,
                    self.now,
                    &guard,
                    &live,
                    self.events.values,
                    &mut self.events.mutation,
                    &mut self.events.original_work,
                    self.events.world_events,
                )
                .map_err(RootModuleExecutionErrorV1::Body)?;
                build_select::select_due_builds_v1(
                    &mut step,
                    current,
                    parent,
                    self.members,
                    materials,
                    sources,
                    work,
                    normal
                        .common_mut()
                        .map_err(RootModuleExecutionErrorV1::Body)?,
                    &live,
                )
                .map_err(RootModuleExecutionErrorV1::Body)?
            };
            for due in order.iter() {
                let live = live_owner();
                let guard = self.ctx.transaction.guard();
                let prepared = {
                    let mut step = IwsStepContext::borrow_counted_at(
                        self.ctx.transaction,
                        self.now,
                        &guard,
                        &live,
                        self.events.values,
                        &mut self.events.mutation,
                        &mut self.events.original_work,
                        self.events.world_events,
                    )
                    .map_err(RootModuleExecutionErrorV1::Body)?;
                    crate::world::incremental_step::factory_schedule_write_io::paid_with_tail(&mut step,current,parent,self.members,materials,work,normal.common_mut().map_err(RootModuleExecutionErrorV1::Body)?,&live,|step,current,parent,_write,materials,work,tail|{
                    // The indexed path never scans the parent again. Agent
                    // presence retains the actual Business parent reader.
                    let mut agent_parent=WorldFieldReadContextV1{root:sources.roots.state_root(oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business),state:parent.state,values:ValueReadContext{source:parent.values.source,memory:parent.values.memory,class:parent.values.class,io:&mut *parent.values.io},key_work:&mut *parent.key_work,semantic_bytes:parent.semantic_bytes,record_bytes:parent.record_bytes,take_state_error:&mut parent.take_state_error,cancelled:&mut parent.cancelled};
                    let guard=step.transaction.guard();
                    crate::world::incremental_normal_tail::factory_built::complete_current_factory_build_from_index_v1(step,materials,&sources.lifecycle,current,&mut agent_parent,sources.roots,sources.builds,&due.identity,&due.selected,tail,&mut work.lifecycle,&guard,&live)
                }).map_err(RootModuleExecutionErrorV1::Body)?
                };
                let event = self.persist_due_factory_built_v1(
                    prepared,
                    materials,
                    normal
                        .common_mut()
                        .map_err(RootModuleExecutionErrorV1::Body)?,
                    live_owner,
                )?;
                deferred
                    .push(
                        event,
                        self.ctx.transaction,
                        normal
                            .common_mut()
                            .map_err(RootModuleExecutionErrorV1::Body)?,
                        self.now,
                        &live_owner(),
                    )
                    .map_err(RootModuleExecutionErrorV1::Body)?;
            }
            Ok(())
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }
}
