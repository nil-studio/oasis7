//! Complete fixed material-transit phase. Reuses the original continuous lane
//! adapter, snapshots due ordering before mutations, then persists every result
//! before routing the original batch in FIFO order.
use super::*;
use crate::world::{
    incremental_normal_action_factory_schedule::FactoryScheduleSourcesV1,
    incremental_normal_industry_due::deferred::DueEventQueueV1,
};
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: oasis7_formal_protocol::incremental_material::OriginalObjectStreamReader,
{
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn execute_normal_material_transits_v1<P, CE, CC, PE, PC, IE, IC, ME>(
        &mut self,
        parent: &BoundNormalTickParentV1<'_>,
        sources: &FactoryScheduleSourcesV1<'_>,
        work: &mut crate::world::incremental_action_factory_lifecycle::FactoryLifecycleWorkV1,
        logistics_work: &mut LogisticsWorkBudgetV1,
        industry_read: &mut IndustryFamilyReadContextV1<'_, M, R, IE, IC>,
        materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
        current: &mut WorldFieldReadContextV1<'_, P, R, CE, CC>,
        agent_parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
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
        IE: FnMut() -> Option<ValueTreeError>,
        IC: FnMut() -> bool,
        ME: FnMut() -> Option<ValueTreeError>,
    {
        let result = (|| {
            let live = live_owner();
            let guard = self.ctx.transaction.guard();
            normal
                .charge(4096)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            normal
                .validate_parent(parent)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            NormalTailSourcesV1::from_bound_parent(parent)
                .validate(
                    self.ctx.transaction,
                    self.events.mutation.memory,
                    &guard,
                    &live,
                )
                .map_err(|e| RootModuleExecutionErrorV1::Body(e.into()))?;
            if !directory.uses_budget(normal.memory())
                || !materials.parent.uses_budget(normal.memory())
            {
                return Err(RootModuleExecutionErrorV1::Body(
                    BodyMutationError::Invalid("material transit phase source pool"),
                ));
            }
            // The reader is caller-owned and continuous: its genuine qualified visitor
            // budget, cancellation and outer scan are never reconstructed or reset.
            let max_scalar = work.max_scalar_bytes;
            let max_items = work.pending_entries_remaining;
            let memory = self.events.mutation.memory;
            let due=normal.with_child(|tail,lease|{
    tail.charge_record()?;
    let before=selection_lanes(&mut work.industry,industry_read,current,&mut self.events.mutation,Some((lease.remaining_steps(),lease.remaining_read_bytes())))?;
    let collected=(||{
     let guard=self.ctx.transaction.guard();let mut step=IwsStepContext::borrow_counted_at(self.ctx.transaction,self.now,&guard,&live,self.events.values,&mut self.events.mutation,&mut self.events.original_work,self.events.world_events)?;
     crate::world::incremental_normal_tail::transits::collect_due_transit_order_v1(sources.roots,sources.lifecycle.industry,industry_read,&mut work.industry,parent,&mut step,current,memory,tail,self.now,max_scalar,max_items,&guard,&live)
    })();
    let after=selection_lanes(&mut work.industry,industry_read,current,&mut self.events.mutation,None)?;
    let accounted=lane_delta(before,after,lease);match(collected,accounted){(Err(first),_)=>Err(first),(Ok(_),Err(error))=>Err(error),(Ok(value),Ok(()))=>Ok(value)}
   }).map_err(RootModuleExecutionErrorV1::Body)?;
            let mut queue = DueEventQueueV1::new(
                self.ctx.transaction,
                normal
                    .common_mut()
                    .map_err(RootModuleExecutionErrorV1::Body)?,
                self.now,
                &live_owner(),
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            for selected in due.entries() {
                let live = live_owner();
                let prepared=normal.with_child(|tail,lease|{
     tail.charge_record()?;
     let before=reducer_lanes(work,logistics_work,current,agent_parent,materials,&mut self.events.mutation,Some((lease.remaining_steps(),lease.remaining_read_bytes())))?;
     let reduced=(||{
      let guard=self.ctx.transaction.guard();let mut step=IwsStepContext::borrow_counted_at(self.ctx.transaction,self.now,&guard,&live,self.events.values,&mut self.events.mutation,&mut self.events.original_work,self.events.world_events)?;
      crate::world::incremental_normal_tail::transits::reduce_current_material_transit_v1(selected.job_id,&mut step,materials,parent,sources.roots,current,agent_parent,sources.lifecycle.factories,sources.lifecycle.index,sources.lifecycle.industry,sources.logistics,sources.lifecycle.agents,sources.lifecycle.native,work,logistics_work,tail,&guard,&live)
     })();
     let after=reducer_lanes(work,logistics_work,current,agent_parent,materials,&mut self.events.mutation,None)?;
     let accounted=lane_delta(before,after,lease);match(reduced,accounted){(Err(first),_)=>Err(first),(Ok(_),Err(error))=>Err(error),(Ok(value),Ok(()))=>Ok(value)}
    }).map_err(RootModuleExecutionErrorV1::Body)?;
                // Only the actual reducer's private source may be counted. Keep its fee and
                // typed counted note alive until the original batch routing has finished.
                let retained = self.persist_due_material_transit_v1(
                    prepared,
                    materials,
                    normal
                        .common_mut()
                        .map_err(RootModuleExecutionErrorV1::Body)?,
                    live_owner,
                )?;
                queue
                    .push(
                        retained,
                        self.ctx.transaction,
                        normal
                            .common_mut()
                            .map_err(RootModuleExecutionErrorV1::Body)?,
                        self.now,
                        &live_owner(),
                    )
                    .map_err(RootModuleExecutionErrorV1::Body)?;
            }
            self.flush_due_events_v1(
                &mut queue,
                normal
                    .common_mut()
                    .map_err(RootModuleExecutionErrorV1::Body)?,
                directory,
                enumeration,
                live_owner,
            )?;
            self.ctx
                .transaction
                .validate_guard(&self.ctx.transaction.guard(), &live_owner())?;
            Ok(())
        })();
        if result.is_err() {
            abort_preserving_first(self.ctx.transaction);
        }
        result
    }
}
