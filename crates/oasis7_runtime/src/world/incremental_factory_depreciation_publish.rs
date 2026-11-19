//! Counted original publication and actual immediate routing for depreciation.
use super::{
    PreparedFactoryDepreciationV1,
    reduce::{
        AppliedFactoryDepreciationV1, FactoryDepreciationReduceErrorV1,
        FactoryDepreciationReducerV1,
    },
};
use crate::world::{
    incremental_allocators::RuntimeAllocatorV1,
    incremental_body::BodyMutationError,
    incremental_body_event::AgentEventOriginalV1,
    incremental_module_invocation::{
        AcceptedInvocationDirectoryV1, CurrentInvocationEnumerationWorkV1,
        RootModuleExecutionContextV1, RootModuleExecutionErrorV1,
    },
};
use crate::{DomainEvent, WorldEvent, WorldEventBody};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_material::OriginalObjectStreamReader,
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
#[derive(Debug)]
pub(in crate::world) enum FactoryDepreciationPublishErrorV1<OE> {
    Reduce(FactoryDepreciationReduceErrorV1),
    Root(RootModuleExecutionErrorV1<OE>),
    Body(BodyMutationError),
    Value(ValueTreeError),
    Execution(ExecutionError),
}
impl<OE> From<ValueTreeError> for FactoryDepreciationPublishErrorV1<OE> {
    fn from(e: ValueTreeError) -> Self {
        Self::Value(e)
    }
}
impl<OE> From<ExecutionError> for FactoryDepreciationPublishErrorV1<OE> {
    fn from(e: ExecutionError) -> Self {
        Self::Execution(e)
    }
}
impl<OE> From<BodyMutationError> for FactoryDepreciationPublishErrorV1<OE> {
    fn from(e: BodyMutationError) -> Self {
        Self::Body(e)
    }
}
impl<OE> From<FactoryDepreciationReduceErrorV1> for FactoryDepreciationPublishErrorV1<OE> {
    fn from(e: FactoryDepreciationReduceErrorV1) -> Self {
        Self::Reduce(e)
    }
}
/// Private candidate constructor below consumes an authentic applied item and
/// the real allocator preview. The shared sink cannot accept arbitrary DTOs.
pub(in crate::world) struct PreparedFactoryDepreciationEventV1 {
    event: ReservedValue<WorldEvent>,
    guard: PreparedGuard,
    now: u64,
    _fee: MemoryReservation,
}
impl PreparedFactoryDepreciationEventV1 {
    pub(in crate::world) fn event(&self) -> &WorldEvent {
        &self.event
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_guard(&self.guard, live)?;
        if !tx.uses_budget(memory)
            || !self.event.uses_budget(memory)
            || self.now != now
            || self.event.time != now
            || self.event.caused_by.is_some()
            || !matches!(
                self.event.body,
                WorldEventBody::Domain(DomainEvent::FactoryDurabilityChanged { .. })
            )
        {
            return Err(BodyMutationError::Invalid(
                "prepared depreciation event binding",
            ));
        }
        Ok(())
    }
}
/// Only minted below after the actual shared original/fact/counter sink succeeds.
pub(in crate::world) struct PublishedFactoryDepreciationV1<'batch> {
    event: ReservedValue<WorldEvent>,
    batch: &'batch ReservedValue<PreparedFactoryDepreciationV1>,
    ordinal: usize,
    identity: PreparedGuard,
    _fee: MemoryReservation,
}
impl PublishedFactoryDepreciationV1<'_> {
    pub(in crate::world) fn event(&self) -> &WorldEvent {
        &self.event
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
    ) -> Result<(), ExecutionError> {
        tx.validate_transaction_identity(&self.identity)
    }
}
/// No constructor outside this module: produced only AFTER actual aggregate
/// event routing returns success. A persisted event alone cannot advance items.
pub(in crate::world) struct RoutedFactoryDepreciationV1<'batch> {
    batch: &'batch ReservedValue<PreparedFactoryDepreciationV1>,
    ordinal: usize,
    guard: PreparedGuard,
    _fee: MemoryReservation,
}
impl<'batch> RoutedFactoryDepreciationV1<'batch> {
    pub(super) fn ordinal(&self) -> usize {
        self.ordinal
    }
    pub(super) fn batch(&self) -> &'batch ReservedValue<PreparedFactoryDepreciationV1> {
        self.batch
    }
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        live: &ExecutionFence,
    ) -> Result<(), FactoryDepreciationReduceErrorV1> {
        tx.validate_guard(&self.guard, live)?;
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
    /// No callback may stand in for routing. The genuine persisted token enters
    /// the actual Current aggregate consumer before this private progress advances.
    pub(in crate::world) fn publish_route_depreciation_item_v1<'batch>(
        &mut self,
        reducer: &mut FactoryDepreciationReducerV1<'batch>,
        applied: AppliedFactoryDepreciationV1<'batch>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<PublishedFactoryDepreciationV1<'batch>, FactoryDepreciationPublishErrorV1<O::Error>>
    {
        let result = (|| {
            let live = live_owner();
            applied.validate(
                self.ctx.transaction,
                self.ctx.values.memory,
                self.now,
                &live,
            )?;
            if self.ctx.values.class != MemoryClass::Ordinary
                || !self
                    .events
                    .mutation
                    .memory
                    .same_pool(self.ctx.values.memory)
            {
                return Err(ValueTreeError::Invalid("depreciation publication pool/class").into());
            }
            if (self.ctx.cancelled)() || (self.events.mutation.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let WorldEventBody::Domain(DomainEvent::FactoryDurabilityChanged {
                factory_id,
                previous_durability_ppm,
                durability_ppm,
                reason,
            }) = applied.body()
            else {
                return Err(ValueTreeError::Invalid("applied depreciation body").into());
            };
            // Exhaustive closed event variant: two String clones, no arbitrary Clone
            // capability or max-encoded-bytes surrogate admission.
            let bytes = std::mem::size_of::<WorldEvent>()
                .checked_add(factory_id.len())
                .and_then(|n| n.checked_add(reason.len()))
                .ok_or(ValueTreeError::ResourceLimited(
                    "depreciation original DTO bound",
                ))?;
            let event_fee = self
                .ctx
                .values
                .memory
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            let metadata_fee = self
                .ctx
                .values
                .memory
                .reserve(
                    std::mem::size_of::<PreparedFactoryDepreciationEventV1>()
                        + std::mem::size_of::<PublishedFactoryDepreciationV1<'_>>()
                        + 128,
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            let route_fee = self
                .ctx
                .values
                .memory
                .reserve(
                    std::mem::size_of::<RoutedFactoryDepreciationV1<'_>>() + 128,
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            crate::world::incremental_clock::check_clock(
                self.now,
                self.ctx.transaction,
                &mut self.events.mutation,
            )?;
            let sequence = crate::world::World::read_iws_allocator(
                RuntimeAllocatorV1::Event,
                self.ctx.transaction,
                self.events.values,
                &mut self.events.mutation,
            )?;
            let (id, _, _) =
                crate::world::World::preview_next_event_id(sequence.next, sequence.era);
            let factory_copy = factory_id.to_owned();
            let reason_copy = reason.to_owned();
            if factory_copy.capacity() > factory_id.len() || reason_copy.capacity() > reason.len() {
                return Err(
                    ValueTreeError::ResourceLimited("depreciation original DTO capacity").into(),
                );
            }
            let prepared = PreparedFactoryDepreciationEventV1 {
                event: event_fee.retain(WorldEvent {
                    id,
                    time: self.now,
                    caused_by: None,
                    body: WorldEventBody::Domain(DomainEvent::FactoryDurabilityChanged {
                        factory_id: factory_copy,
                        previous_durability_ppm: *previous_durability_ppm,
                        durability_ppm: *durability_ppm,
                        reason: reason_copy,
                    }),
                }),
                guard: self.ctx.transaction.guard(),
                now: self.now,
                _fee: metadata_fee,
            };
            let live = live_owner();
            crate::world::incremental_module_execution_event::contribute_factory_depreciation_event_v1(self.now,&prepared,&live,self.ctx.transaction,self.events)?;
            if (self.ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let published = PublishedFactoryDepreciationV1 {
                event: prepared.event,
                batch: applied.batch(),
                ordinal: applied.ordinal(),
                identity: self.ctx.transaction.guard(),
                _fee: prepared._fee,
            };
            let token=crate::world::incremental_module_execution_event::PersistedWorldEventV1::from_factory_depreciation(&published);
            self.route_persisted_event_v1(&token, None, directory, enumeration, live_owner)
                .map_err(FactoryDepreciationPublishErrorV1::Root)?;
            if (self.ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let live = live_owner();
            let guard = self.ctx.transaction.guard();
            self.ctx.transaction.validate_guard(&guard, &live)?;
            let routed = RoutedFactoryDepreciationV1 {
                batch: published.batch,
                ordinal: published.ordinal,
                guard: self.ctx.transaction.guard(),
                _fee: route_fee,
            };
            reducer.finish_routed_v1(&routed, self.ctx.transaction, &guard, &live)?;
            Ok(published)
        })();
        if result.is_err() {
            let _: Result<(), ExecutionError> = self
                .ctx
                .transaction
                .contribute(|_| Err(ExecutionError::Aborted));
        }
        result
    }
}
