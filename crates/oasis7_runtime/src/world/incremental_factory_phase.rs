//! One complete factory depreciation phase in the caller's sole counted tick.
//! Accepted caches are supplied, never silently rebuilt on an ordinary tick.
use crate::world::{
    incremental_body::BodyMutationError,
    incremental_body_event::AgentEventOriginalV1,
    incremental_factory_depreciation::{
        FactoryDepreciationErrorV1, FactoryDepreciationWorkV1,
        prepare_current_factory_depreciation_v1,
        publish::FactoryDepreciationPublishErrorV1,
        reduce::{
            FactoryDepreciationReduceErrorV1, FactoryDepreciationReduceWorkV1,
            start_factory_depreciation_reducer_v1,
        },
    },
    incremental_factory_directory_orders::*,
    incremental_factory_directory_records::*,
    incremental_industrial_eligibility::{
        IndustrialEligibilitySourcesV1, IndustrialEligibilityWorkV1,
    },
    incremental_module_invocation::{
        AcceptedInvocationDirectoryV1, CurrentInvocationEnumerationWorkV1,
        RootModuleExecutionContextV1,
    },
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, PreparedGuard},
    incremental_material::OriginalObjectStreamReader,
    incremental_state::{MerkleNodeReader, StatePageWriter, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_agent_authority_records::AgentAuthorityFamilyQualificationV1,
    incremental_factory_authority_records::FactoryAuthorityFamilyQualificationV1,
    incremental_factory_records::*, incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1, incremental_industry_jobs::*,
    incremental_inventory::*,
    incremental_runtime_artifact_resources::RuntimeArtifactResourcesQualificationV1,
    incremental_runtime_revocation::RuntimeRevocationQualificationV1,
    incremental_world_native::WorldNativeFieldsQualificationV1,
    incremental_world_rule_composite::WorldRuleCompositeReadWorkV1,
    incremental_world_rule_composite_family::bind_current_agent_orders_v1,
    incremental_world_rule_receipts::AgentOrdersQualificationV1,
};
/// Each referenced type has private authenticated constructors. No raw ID list,
/// caller RootSet or record count can stand in for a complete parent cache.
pub(in crate::world) struct FactoryPhaseAcceptedCachesV1<'a> {
    pub orders: &'a ReservedValue<AcceptedFactoryOrderDirectoryV1>,
    pub records: &'a ReservedValue<AcceptedFactoryRecipeDirectoryV1>,
    pub materials: &'a ReservedValue<ParentInventoryMaterialScopeV1>,
    pub invocations: &'a ReservedValue<AcceptedInvocationDirectoryV1>,
}
pub(in crate::world) struct FactoryPhaseFamiliesV1<'a> {
    pub orders: &'a ReservedValue<AgentOrdersQualificationV1>,
    pub native: &'a ReservedValue<WorldNativeFieldsQualificationV1>,
    pub factories: &'a ReservedValue<FactoryFamilyQualificationV1>,
    pub recipes: &'a ReservedValue<IndustryFamilyQualificationV1>,
    pub sites: &'a ReservedValue<FactoryAuthorityFamilyQualificationV1>,
    pub anchors: &'a ReservedValue<AgentAuthorityFamilyQualificationV1>,
    pub revocation: &'a ReservedValue<RuntimeRevocationQualificationV1>,
    pub artifacts: &'a ReservedValue<RuntimeArtifactResourcesQualificationV1>,
}
pub(in crate::world) struct FactoryPhaseWorkV1 {
    pub orders: FactoryOrderDirectoryWorkV1,
    pub records: FactoryRecipeDirectoryWorkV1,
    pub material_family: InventoryFamilyWorkV1,
    pub material_scope: InventoryMaterialScopeWorkV1,
    pub factory: FactoryWorkBudgetV1,
    pub recipe: IndustryWorkBudgetV1,
    pub industrial: IndustrialEligibilityWorkV1,
    pub prepare: FactoryDepreciationWorkV1,
    pub reduce: FactoryDepreciationReduceWorkV1,
    pub group: InventoryWorldGroupWorkV1,
    pub enumeration: CurrentInvocationEnumerationWorkV1,
    pub scan: StateScanBudget,
    pub typed: WorldRuleCompositeReadWorkV1,
}
#[derive(Debug)]
pub(in crate::world) enum FactoryPhaseErrorV1<OE> {
    Execution(ExecutionError),
    Value(ValueTreeError),
    Overlay(WorldFieldOverlayErrorV1),
    Directory(FactoryOrderDirectoryErrorV1),
    Prepare(FactoryDepreciationErrorV1),
    Reduce(FactoryDepreciationReduceErrorV1),
    Publication(FactoryDepreciationPublishErrorV1<OE>),
    Body(BodyMutationError),
}
macro_rules! from_error {
    ($ty:ty,$variant:ident) => {
        impl<OE> From<$ty> for FactoryPhaseErrorV1<OE> {
            fn from(e: $ty) -> Self {
                Self::$variant(e)
            }
        }
    };
}
from_error!(ExecutionError, Execution);
from_error!(ValueTreeError, Value);
from_error!(WorldFieldOverlayErrorV1, Overlay);
from_error!(FactoryOrderDirectoryErrorV1, Directory);
from_error!(FactoryDepreciationErrorV1, Prepare);
from_error!(FactoryDepreciationReduceErrorV1, Reduce);
from_error!(BodyMutationError, Body);
#[derive(Debug)]
pub(in crate::world) struct FactoryPhaseOutcomeV1 {
    pub resumed: bool,
    pub prepared_events: usize,
    pub published_and_routed_events: usize,
    completion: FactoryPrologueCompletionV1,
}
struct FactoryPrologueCompletionV1 {
    now: u64,
    identity: PreparedGuard,
    counter: crate::world::incremental_world_event_counter::WorldEventCounterV1,
    memory: MemoryBudget,
    _fee: MemoryReservation,
}
impl std::fmt::Debug for FactoryPrologueCompletionV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FactoryPrologueCompletionV1")
            .field("now", &self.now)
            .finish_non_exhaustive()
    }
}
impl FactoryPhaseOutcomeV1 {
    /// A genuine completed phase binds the next FIFO consumer to this same
    /// transaction, counter and tick. It grants no consensus or resume authority.
    pub(in crate::world) fn validate_for_queue<M: MerkleNodeReader>(
        &self,
        tx: &mut oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        counter: &crate::world::incremental_world_event_counter::WorldEventCounterV1,
        now: u64,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.contribute(|tx| {
            tx.validate_guard(&tx.guard(), live)?;
            tx.validate_transaction_identity(&self.completion.identity)?;
            self.completion.counter.validate(tx)?;
            if self.resumed
                || self.completion.now != now
                || !self.completion.memory.same_pool(memory)
                || !tx.uses_budget(memory)
                || !counter.same_counter(&self.completion.counter)
            {
                return Err(BodyMutationError::Invalid(
                    "completed factory prologue queue binding",
                ));
            }
            Ok(())
        })
    }
}
// Only the real counted-clock start below can construct this local permit.
// It proves transaction/tick sequencing, never consensus or resume authority.
struct FactoryProloguePermitV1 {
    now: u64,
    identity: PreparedGuard,
    counter: crate::world::incremental_world_event_counter::WorldEventCounterV1,
    memory: MemoryBudget,
    completion_fee: MemoryReservation,
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
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn begin_current_factory_prologue_v1<P, V, ER, CR, FR, FC, RR, RC>(
        &mut self,
        caches: &FactoryPhaseAcceptedCachesV1<'_>,
        families: &FactoryPhaseFamiliesV1<'_>,
        expected_time: u64,
        read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
        factory: &mut FactoryTransitionReadContextV1<'_, FR, FC>,
        recipe: &mut IndustryTransitionReadContextV1<'_, RR, RC>,
        work: &mut FactoryPhaseWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<FactoryPhaseOutcomeV1, FactoryPhaseErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        V: ValueNodeReader,
        ER: FnMut() -> Option<ValueTreeError>,
        CR: FnMut() -> bool,
        FR: ValueNodeReader,
        FC: FnMut() -> bool,
        RR: ValueNodeReader,
        RC: FnMut() -> bool,
    {
        let result = (|| {
            let live = live_owner();
            self.ctx
                .transaction
                .validate_guard(&self.ctx.transaction.guard(), &live)?;
            if (self.ctx.cancelled)() || (self.events.mutation.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if self.ctx.values.class != MemoryClass::Ordinary
                || !self.ctx.transaction.uses_budget(self.ctx.values.memory)
                || !self
                    .events
                    .mutation
                    .memory
                    .same_pool(self.ctx.values.memory)
            {
                return Err(ValueTreeError::Invalid("factory prologue pool/class").into());
            }
            if expected_time.checked_add(1) != Some(self.now) {
                return Err(
                    ValueTreeError::Invalid("factory prologue counted tick mismatch").into(),
                );
            }
            // Reserve the opaque completion carrier before any clock or
            // original publication. Successful phase completion only moves it.
            let completion_fee = self
                .ctx
                .values
                .memory
                .reserve(
                    std::mem::size_of::<FactoryPrologueCompletionV1>() + 256,
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            self.events
                .world_events
                .claim_factory_prologue(self.ctx.transaction)?;
            let guard = self.ctx.transaction.guard();
            let step = crate::world::incremental_step::IwsStepContext::begin_counted(
                self.ctx.transaction,
                expected_time,
                &guard,
                &live,
                self.events.values,
                &mut self.events.mutation,
                &mut self.events.original_work,
                self.events.world_events,
            )?;
            let permit = FactoryProloguePermitV1 {
                now: step.time(),
                identity: step.transaction.guard(),
                counter: self.events.world_events.clone(),
                memory: step.mutation.memory.clone(),
                completion_fee,
            };
            drop(step);
            self.run_current_factory_phase_v1(
                caches, families, permit, read, factory, recipe, work, live_owner,
            )
        })();
        if result.is_err() {
            let _ = self
                .ctx
                .transaction
                .contribute::<(), ExecutionError>(|_| Err(ExecutionError::Aborted));
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn run_current_factory_phase_v1<P, V, ER, CR, FR, FC, RR, RC>(
        &mut self,
        caches: &FactoryPhaseAcceptedCachesV1<'_>,
        families: &FactoryPhaseFamiliesV1<'_>,
        permit: FactoryProloguePermitV1,
        read: &mut WorldFieldReadContextV1<'_, P, V, ER, CR>,
        factory: &mut FactoryTransitionReadContextV1<'_, FR, FC>,
        recipe: &mut IndustryTransitionReadContextV1<'_, RR, RC>,
        work: &mut FactoryPhaseWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<FactoryPhaseOutcomeV1, FactoryPhaseErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        V: ValueNodeReader,
        ER: FnMut() -> Option<ValueTreeError>,
        CR: FnMut() -> bool,
        FR: ValueNodeReader,
        FC: FnMut() -> bool,
        RR: ValueNodeReader,
        RC: FnMut() -> bool,
    {
        let result = (|| {
            let live = live_owner();
            self.ctx
                .transaction
                .validate_guard(&self.ctx.transaction.guard(), &live)?;
            if self.ctx.values.class != MemoryClass::Ordinary
                || !self.ctx.transaction.uses_budget(self.ctx.values.memory)
                || !self
                    .events
                    .mutation
                    .memory
                    .same_pool(self.ctx.values.memory)
            {
                return Err(ValueTreeError::Invalid("factory phase pool/class").into());
            }
            self.events.world_events.validate(self.ctx.transaction)?;
            if (self.ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            self.ctx
                .transaction
                .validate_transaction_identity(&permit.identity)?;
            if permit.now != self.now {
                return Err(ValueTreeError::Invalid("factory prologue private permit tick").into());
            }
            let guard = self.ctx.transaction.guard();
            let orders_source = bind_current_agent_orders_v1(
                families.orders,
                self.families.roots,
                self.state,
                self.ctx,
                &mut work.scan,
                &mut work.typed,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            let orders = open_current_factory_order_directory_v1(
                caches.orders,
                &orders_source,
                self.state,
                self.ctx,
                &mut work.scan,
                &mut work.typed,
                &mut work.orders,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            let records = open_current_factory_recipe_directory_v1(
                caches.records,
                families.factories,
                families.recipes,
                self.ctx.transaction,
                factory,
                recipe,
                &mut work.factory,
                &mut work.recipe,
                &mut work.records,
                &guard,
                &live,
            )?;
            let sources = IndustrialEligibilitySourcesV1 {
                roots: self.families.roots,
                orders: &orders,
                agents: self.families.agents,
                native: families.native,
                factories: families.factories,
                sites: families.sites,
                anchors: families.anchors,
                revocation: families.revocation,
                artifacts: families.artifacts,
            };
            let guard = self.ctx.transaction.guard();
            let batch = prepare_current_factory_depreciation_v1(
                &records,
                &sources,
                self.now,
                false,
                self.state,
                self.ctx,
                read,
                factory,
                recipe,
                &mut work.factory,
                &mut work.recipe,
                &mut work.records,
                &mut work.industrial,
                &mut work.prepare,
                &guard,
                &live,
            )?;
            let prepared_events = batch.bodies().len();
            let guard = self.ctx.transaction.guard();
            let mut reducer =
                start_factory_depreciation_reducer_v1(&batch, self.ctx, self.now, &guard, &live)?;
            while reducer.remaining_items() != 0 {
                let live = live_owner();
                let guard = self.ctx.transaction.guard();
                let materials = open_current_inventory_material_scope_v1(
                    caches.materials,
                    self.families.roots,
                    self.state,
                    self.ctx,
                    &mut work.material_family,
                    &mut work.material_scope,
                    &guard,
                    &live,
                )?;
                let guard = self.ctx.transaction.guard();
                let applied = reducer
                    .apply_next_v1(
                        &materials,
                        self.ctx,
                        self.members,
                        &mut work.factory,
                        &mut work.group,
                        &mut work.reduce,
                        &guard,
                        &live,
                    )?
                    .ok_or(ValueTreeError::Invalid(
                        "factory phase premature completion",
                    ))?;
                self.publish_route_depreciation_item_v1(
                    &mut reducer,
                    applied,
                    caches.invocations,
                    &mut work.enumeration,
                    live_owner,
                )
                .map_err(FactoryPhaseErrorV1::Publication)?;
            }
            if (self.ctx.cancelled)() || (read.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            Ok(FactoryPhaseOutcomeV1 {
                resumed: false,
                prepared_events,
                published_and_routed_events: reducer.applied_items(),
                completion: FactoryPrologueCompletionV1 {
                    now: permit.now,
                    identity: self.ctx.transaction.guard(),
                    counter: permit.counter,
                    memory: permit.memory,
                    _fee: permit.completion_fee,
                },
            })
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
