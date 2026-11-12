//! Closed system-event publication input, minted from completed reducers.
//! The borrowed event keeps the reducer's original paid payload alive.
use super::{
    incremental_body::BodyMutationError,
    incremental_normal_tail::factory_built::PreparedNormalFactoryBuiltV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::MerkleNodeReader,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
pub(in crate::world) struct PreparedNormalSystemEventV1<'event> {
    event: &'event crate::WorldEvent,
    identity: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    _outer: ReservedValue<()>,
}
impl<'event> PreparedNormalSystemEventV1<'event> {
    pub(in crate::world) fn from_factory_built<M: MerkleNodeReader>(
        source: &'event PreparedNormalFactoryBuiltV1,
        tx: &ExecutionTransaction<'_, M>,
        now: u64,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Self, BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        source.validate(tx, common.memory(), now, common.work().identity(), live)?;
        if source.event().caused_by.is_some()
            || !matches!(
                source.event().body,
                crate::WorldEventBody::Domain(crate::DomainEvent::FactoryBuilt { .. })
            )
        {
            return Err(BodyMutationError::Invalid(
                "normal FactoryBuilt event source",
            ));
        }
        let fee = common
            .memory()
            .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(Self {
            event: source.event(),
            identity: tx.guard(),
            core: common.work().identity(),
            memory: common.memory().clone(),
            _outer: fee.retain(()),
        })
    }
    pub(in crate::world) fn from_material_transit<M: MerkleNodeReader>(
        source: &'event super::incremental_normal_tail::transits::PreparedNormalMaterialTransitV1,
        tx: &ExecutionTransaction<'_, M>,
        now: u64,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<Self, BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        source.validate(tx, common.memory(), now, common.work().identity(), live)?;
        if source.event().caused_by.is_some()
            || !matches!(
                source.event().body,
                crate::WorldEventBody::Domain(crate::DomainEvent::MaterialTransitCompleted { .. })
            )
        {
            return Err(BodyMutationError::Invalid(
                "normal MaterialTransit event source",
            ));
        }
        let fee = common
            .memory()
            .reserve(std::mem::size_of::<Self>(), MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(Self {
            event: source.event(),
            identity: tx.guard(),
            core: common.work().identity(),
            memory: common.memory().clone(),
            _outer: fee.retain(()),
        })
    }
    pub(in crate::world) fn event(&self) -> &'event crate::WorldEvent {
        self.event
    }
    pub(in crate::world) fn validate_common(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        if self.core != common.work().identity() || !self.memory.same_pool(common.memory()) {
            return Err(BodyMutationError::Invalid(
                "normal system event Common source",
            ));
        }
        Ok(())
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.identity)?;
        if !tx.uses_budget(memory)
            || !self.memory.same_pool(memory)
            || !self._outer.uses_budget(memory)
            || self.event.time != now
            || self.event.caused_by.is_some()
        {
            return Err(BodyMutationError::Invalid(
                "normal system event transaction",
            ));
        }
        Ok(())
    }
}
