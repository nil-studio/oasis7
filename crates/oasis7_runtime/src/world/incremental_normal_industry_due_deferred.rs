//! Original due-economy batch flush points. Only real counted publications
//! enter this queue; retained producer owners keep their exact event data paid.
use super::*;
use crate::world::{
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_module_invocation::{
        AcceptedInvocationDirectoryV1, CurrentInvocationEnumerationWorkV1,
        RootModuleExecutionContextV1, RootModuleExecutionErrorV1,
    },
    incremental_world_event_counter::CountedWorldEventSourceV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_material::OriginalObjectStreamReader,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use std::mem::size_of;
fn invalid() -> BodyMutationError {
    BodyMutationError::Invalid("industrial deferred counted route")
}
fn charge(common: &mut FormalBoundedContextV1<'_>, bytes: usize) -> Result<(), BodyMutationError> {
    common
        .charge(1, bytes as u64)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))
}
enum EventOwnerV1 {
    Build(crate::world::incremental_normal_tail::factory_built::PreparedNormalFactoryBuiltV1),
    Complete(PreparedNormalRecipeCompletedV1),
    Failure(PreparedNormalRecipeFailureV1),
    Attempt(PreparedNormalProductAttemptV1),
}
impl EventOwnerV1 {
    fn event(&self) -> &crate::WorldEvent {
        match self {
            Self::Build(v) => v.event(),
            Self::Complete(v) => v.event(),
            Self::Failure(v) => v.event(),
            Self::Attempt(v) => v.event(),
        }
    }
    fn era(&self) -> Option<u64> {
        match self {
            Self::Attempt(v) => Some(v.event_era()),
            _ => None,
        }
    }
}
pub(in crate::world) struct PaidDeferredDueEventV1 {
    owner: EventOwnerV1,
    counted: CountedWorldEventSourceV1,
    identity: WorkIdentityV1,
    memory: MemoryBudget,
    now: u64,
    _phase: MemoryReservation,
}
impl PaidDeferredDueEventV1 {
    pub(in crate::world) fn product_attempt(&self) -> Option<&PreparedNormalProductAttemptV1> {
        match &self.owner {
            EventOwnerV1::Attempt(value) => Some(value),
            _ => None,
        }
    }
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        self.owner.event()
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        charge(common, 0)?;
        self.counted.validate(tx)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.identity != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || !tx.uses_budget(common.memory())
            || self.now != now
            || self.event().time != now
        {
            return Err(invalid());
        }
        Ok(())
    }
}
pub(in crate::world) struct DueEventQueueV1 {
    events: Vec<PaidDeferredDueEventV1>,
    buffer: Option<MemoryReservation>,
    identity: WorkIdentityV1,
    memory: MemoryBudget,
    now: u64,
    transaction: PreparedGuard,
    _phase: MemoryReservation,
}
impl DueEventQueueV1 {
    pub(in crate::world) fn new<M: MerkleNodeReader>(
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<Self, BodyMutationError> {
        tx.validate_guard(&tx.guard(), live)?;
        charge(common, size_of::<Self>())?;
        let fee = common
            .memory()
            .reserve(size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(Self {
            events: Vec::new(),
            buffer: None,
            identity: common.work().identity(),
            memory: common.memory().clone(),
            now,
            transaction: tx.guard(),
            _phase: fee,
        })
    }
    fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        charge(common, 0)?;
        tx.validate_transaction_identity(&self.transaction)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.identity != common.work().identity()
            || !self.memory.same_pool(common.memory())
            || self.now != now
            || !tx.uses_budget(common.memory())
        {
            return Err(invalid());
        }
        Ok(())
    }
    pub(in crate::world) fn push<M: MerkleNodeReader>(
        &mut self,
        event: PaidDeferredDueEventV1,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.validate(tx, common, now, live)?;
        event.validate(tx, common, now, live)?;
        if self.events.len() == self.events.capacity() {
            let capacity = self.events.len().checked_add(1).ok_or_else(invalid)?;
            let bytes = capacity
                .checked_mul(size_of::<PaidDeferredDueEventV1>())
                .ok_or_else(invalid)?;
            charge(common, bytes)?;
            let fee = common
                .memory()
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            self.events
                .try_reserve_exact(1)
                .map_err(|_| ValueTreeError::ResourceLimited("industrial deferred route slots"))?;
            if self.events.capacity() != capacity {
                return Err(invalid());
            }
            self.buffer = Some(fee);
        }
        self.events.push(event);
        Ok(())
    }
}
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
    pub(in crate::world) fn persist_due_factory_built_v1<ME>(
        &mut self,
        source: crate::world::incremental_normal_tail::factory_built::PreparedNormalFactoryBuiltV1,
        materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<PaidDeferredDueEventV1, RootModuleExecutionErrorV1<O::Error>>
    where
        ME: FnMut() -> Option<ValueTreeError>,
    {
        let result = (|| {
            charge(common, size_of::<PaidDeferredDueEventV1>())
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let phase = common
                .memory()
                .reserve(size_of::<PaidDeferredDueEventV1>(), MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let live = live_owner();
            let _persisted=crate::world::incremental_module_execution_event::contribute_normal_factory_built_event_v1(self.now,&source,common,&live,self.ctx.transaction,self.events,materials).map_err(RootModuleExecutionErrorV1::Body)?;
            drop(_persisted);
            let counted = self
                .events
                .world_events
                .latest_source(self.ctx.transaction)
                .map_err(RootModuleExecutionErrorV1::Body)?
                .ok_or_else(|| RootModuleExecutionErrorV1::Body(invalid()))?;
            Ok(PaidDeferredDueEventV1 {
                owner: EventOwnerV1::Build(source),
                counted,
                identity: common.work().identity(),
                memory: common.memory().clone(),
                now: self.now,
                _phase: phase,
            })
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }
    pub(in crate::world) fn persist_due_recipe_completed_v1<ME>(
        &mut self,
        source: PreparedNormalRecipeCompletedV1,
        materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<PaidDeferredDueEventV1, RootModuleExecutionErrorV1<O::Error>>
    where
        ME: FnMut() -> Option<ValueTreeError>,
    {
        let result = (|| {
            charge(common, size_of::<PaidDeferredDueEventV1>())
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let phase = common
                .memory()
                .reserve(size_of::<PaidDeferredDueEventV1>(), MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let live = live_owner();
            let _persisted=crate::world::incremental_module_execution_event::contribute_normal_recipe_completed_event_v1(self.now,&source,common,&live,self.ctx.transaction,self.events,materials).map_err(RootModuleExecutionErrorV1::Body)?;
            drop(_persisted);
            let counted = self
                .events
                .world_events
                .latest_source(self.ctx.transaction)
                .map_err(RootModuleExecutionErrorV1::Body)?
                .ok_or_else(|| RootModuleExecutionErrorV1::Body(invalid()))?;
            Ok(PaidDeferredDueEventV1 {
                owner: EventOwnerV1::Complete(source),
                counted,
                identity: common.work().identity(),
                memory: common.memory().clone(),
                now: self.now,
                _phase: phase,
            })
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }
    pub(in crate::world) fn persist_due_recipe_failure_v1<ME>(
        &mut self,
        source: PreparedNormalRecipeFailureV1,
        materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<PaidDeferredDueEventV1, RootModuleExecutionErrorV1<O::Error>>
    where
        ME: FnMut() -> Option<ValueTreeError>,
    {
        let result = (|| {
            charge(common, size_of::<PaidDeferredDueEventV1>())
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let phase = common
                .memory()
                .reserve(size_of::<PaidDeferredDueEventV1>(), MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let live = live_owner();
            let _persisted=crate::world::incremental_module_execution_event::contribute_due_recipe_failure_event_v1(self.now,&source,common,&live,self.ctx.transaction,self.events,materials).map_err(RootModuleExecutionErrorV1::Body)?;
            drop(_persisted);
            let counted = self
                .events
                .world_events
                .latest_source(self.ctx.transaction)
                .map_err(RootModuleExecutionErrorV1::Body)?
                .ok_or_else(|| RootModuleExecutionErrorV1::Body(invalid()))?;
            Ok(PaidDeferredDueEventV1 {
                owner: EventOwnerV1::Failure(source),
                counted,
                identity: common.work().identity(),
                memory: common.memory().clone(),
                now: self.now,
                _phase: phase,
            })
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }
    pub(in crate::world) fn persist_due_product_attempt_v1<ME>(
        &mut self,
        source: PreparedNormalProductAttemptV1,
        materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<PaidDeferredDueEventV1, RootModuleExecutionErrorV1<O::Error>>
    where
        ME: FnMut() -> Option<ValueTreeError>,
    {
        let result = (|| {
            charge(common, size_of::<PaidDeferredDueEventV1>())
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let phase = common
                .memory()
                .reserve(size_of::<PaidDeferredDueEventV1>(), MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let live = live_owner();
            let _persisted=crate::world::incremental_module_execution_event::contribute_due_product_attempt_event_v1(self.now,&source,common,&live,self.ctx.transaction,self.events,materials).map_err(RootModuleExecutionErrorV1::Body)?;
            drop(_persisted);
            let counted = self
                .events
                .world_events
                .latest_source(self.ctx.transaction)
                .map_err(RootModuleExecutionErrorV1::Body)?
                .ok_or_else(|| RootModuleExecutionErrorV1::Body(invalid()))?;
            Ok(PaidDeferredDueEventV1 {
                owner: EventOwnerV1::Attempt(source),
                counted,
                identity: common.work().identity(),
                memory: common.memory().clone(),
                now: self.now,
                _phase: phase,
            })
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }
    pub(in crate::world) fn route_deferred_due_event_v1(
        &mut self,
        source: &PaidDeferredDueEventV1,
        common: &mut FormalBoundedContextV1<'_>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<usize, RootModuleExecutionErrorV1<O::Error>> {
        let result = (|| {
            let live = live_owner();
            source
                .validate(self.ctx.transaction, common, self.now, &live)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let event=crate::world::incremental_module_execution_event::PersistedWorldEventV1::from_deferred_due(source,self.ctx.transaction,common,self.now,&live).map_err(RootModuleExecutionErrorV1::Body)?;
            self.route_persisted_event_v1(
                &event,
                source.owner.era(),
                directory,
                enumeration,
                live_owner,
            )
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }
    pub(in crate::world) fn flush_due_events_v1(
        &mut self,
        queue: &mut DueEventQueueV1,
        common: &mut FormalBoundedContextV1<'_>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<(), RootModuleExecutionErrorV1<O::Error>> {
        let result = (|| {
            queue
                .validate(self.ctx.transaction, common, self.now, &live_owner())
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let events = std::mem::take(&mut queue.events);
            let _buffer = queue.buffer.take();
            for event in events {
                self.route_deferred_due_event_v1(
                    &event,
                    common,
                    directory,
                    enumeration,
                    live_owner,
                )?;
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
