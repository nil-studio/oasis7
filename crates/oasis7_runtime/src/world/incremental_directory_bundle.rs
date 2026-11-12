//! Authentic final Current directory capture and real-seal binding.
//! No history candidate, durable acceptance or head promotion is minted here.
use crate::world::{
    incremental_body::BodyMutationError,
    incremental_body_event::AgentEventOriginalV1,
    incremental_factory_directory_orders::*,
    incremental_factory_directory_records::*,
    incremental_factory_phase::{
        FactoryPhaseAcceptedCachesV1, FactoryPhaseFamiliesV1, FactoryPhaseWorkV1,
    },
    incremental_module_invocation::{
        CurrentInvocationEnumerationErrorV1, FrozenInvocationDirectoryV1,
        RootModuleExecutionContextV1, collect_current_module_invocation_ids_v1,
        freeze_current_invocation_directory_v1,
    },
};
use oasis7_formal_protocol::{
    incremental_execution::{
        ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
        SealedExecutionBindingV1,
    },
    incremental_material::OriginalObjectStreamReader,
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_factory_records::FactoryTransitionReadContextV1,
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_industry_jobs::IndustryTransitionReadContextV1,
    incremental_inventory::{
        FrozenInventoryIdentityDirectoryV1, FrozenInventoryMaterialScopeV1,
        InventoryFamilyQualificationV1, InventoryIdentityDirectoryV1,
        InventoryIdentityDirectoryWorkV1, InventoryMaterialScopeWorkV1,
        ParentInventoryMaterialScopeV1, freeze_current_inventory_identity_directory_v1,
        freeze_current_inventory_material_scope_v1, open_current_inventory_material_scope_v1,
    },
    incremental_world_rule_composite_family::bind_current_agent_orders_v1,
};
#[derive(Debug)]
pub(in crate::world) enum RuntimeDirectoryFreezeErrorV1 {
    Execution(ExecutionError),
    Value(ValueTreeError),
    Directory(FactoryOrderDirectoryErrorV1),
    Invocation(CurrentInvocationEnumerationErrorV1),
    Overlay(WorldFieldOverlayErrorV1),
}
macro_rules! from_error {
    ($ty:ty,$variant:ident) => {
        impl From<$ty> for RuntimeDirectoryFreezeErrorV1 {
            fn from(e: $ty) -> Self {
                Self::$variant(e)
            }
        }
    };
}
from_error!(ExecutionError, Execution);
from_error!(ValueTreeError, Value);
from_error!(FactoryOrderDirectoryErrorV1, Directory);
from_error!(CurrentInvocationEnumerationErrorV1, Invocation);
from_error!(WorldFieldOverlayErrorV1, Overlay);
/// All members are genuine private typed snapshots. Readiness is not a commit
/// permission: only the actual runtime prepare/seal path may later bind this.
pub(in crate::world) struct FrozenRuntimeDirectoryBundleV1 {
    guard: PreparedGuard,
    memory: MemoryBudget,
    orders: ReservedValue<FrozenFactoryOrderDirectoryV1>,
    records: ReservedValue<FrozenFactoryRecipeDirectoryV1>,
    invocations: ReservedValue<FrozenInvocationDirectoryV1>,
    materials: ReservedValue<FrozenInventoryMaterialScopeV1>,
    inventory_identities: ReservedValue<FrozenInventoryIdentityDirectoryV1>,
}
impl FrozenRuntimeDirectoryBundleV1 {
    pub(in crate::world) fn iter_factory_ids(&self) -> impl ExactSizeIterator<Item = &str> {
        self.records.iter_factory_ids()
    }
    pub(in crate::world) fn iter_recipe_job_ids(&self) -> impl ExactSizeIterator<Item = u64> + '_ {
        self.records.iter_recipe_job_ids()
    }
    pub(in crate::world) fn iter_order_indices(
        &self,
    ) -> impl ExactSizeIterator<
        Item = (
            &str,
            &oasis7_world_state::incremental_world_rule_composite_family::CurrentAgentOrderIndexV1,
        ),
    > {
        self.orders.iter_order_indices()
    }

    pub(in crate::world) fn validate_terminal_guard<M: MerkleNodeReader>(
        &self,
        tx: &mut ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), RuntimeDirectoryFreezeErrorV1> {
        let result = (|| {
            tx.validate_guard(&self.guard, live)?;
            if !self.memory.same_pool(memory) || !tx.uses_budget(memory) {
                return Err(ValueTreeError::Invalid("frozen directory bundle pool").into());
            }
            self.orders.validate_guard(tx, memory, live)?;
            self.records.validate_guard(tx, memory, live)?;
            self.invocations.validate_guard(tx, memory, live)?;
            self.materials.validate_guard(tx, memory, live)?;
            self.inventory_identities.validate_guard(tx, memory, live)?;
            Ok(())
        })();
        if result.is_err() {
            let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
        }
        result
    }
    pub(in crate::world) fn validate_sealed(
        &self,
        binding: &SealedExecutionBindingV1,
        roots: &RootSetV1,
        memory: &MemoryBudget,
    ) -> Result<(), RuntimeDirectoryFreezeErrorV1> {
        if !self.memory.same_pool(memory) {
            return Err(ValueTreeError::Invalid("sealed directory bundle pool").into());
        }
        binding.validate_source_guard(&self.guard, memory)?;
        binding.validate_after_roots(roots, memory)?;
        self.orders.validate_sealed(binding, roots, memory)?;
        self.records.validate_sealed(binding, roots, memory)?;
        self.invocations.validate_sealed(binding, roots, memory)?;
        self.materials.validate_sealed(binding, roots, memory)?;
        self.inventory_identities
            .validate_sealed(binding, roots, memory)?;
        Ok(())
    }
}
/// Owning structural output authenticated by the actual seal witness. It is
/// deliberately not an Accepted cache bundle; candidate/VerifiedFormalCommit
/// binding and the real durable Host publication remain separate obligations.
pub(in crate::world) struct SealedRuntimeDirectoryBundleV1 {
    frozen: ReservedValue<FrozenRuntimeDirectoryBundleV1>,
    binding: ReservedValue<SealedExecutionBindingV1>,
    materials: ReservedValue<ParentInventoryMaterialScopeV1>,
    inventory_identities: ReservedValue<InventoryIdentityDirectoryV1>,
}
impl SealedRuntimeDirectoryBundleV1 {
    pub(in crate::world) fn after_roots_digest(&self) -> [u8; 32] {
        self.binding.after_roots_digest()
    }

    pub(in crate::world) fn material_parent(
        &self,
    ) -> &ReservedValue<ParentInventoryMaterialScopeV1> {
        &self.materials
    }
    pub(in crate::world) fn inventory_identity_parent(
        &self,
    ) -> &ReservedValue<InventoryIdentityDirectoryV1> {
        &self.inventory_identities
    }
    pub(in crate::world) fn validate_after_roots(
        &self,
        roots: &RootSetV1,
        memory: &MemoryBudget,
    ) -> Result<(), RuntimeDirectoryFreezeErrorV1> {
        self.frozen.validate_sealed(&self.binding, roots, memory)
    }
}
pub(in crate::world) fn bind_runtime_directories_to_actual_seal_v1(
    frozen: ReservedValue<FrozenRuntimeDirectoryBundleV1>,
    binding: ReservedValue<SealedExecutionBindingV1>,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    work: &mut InventoryMaterialScopeWorkV1,
    identity_work: &mut InventoryIdentityDirectoryWorkV1,
    mut cancelled: impl FnMut() -> bool,
) -> Result<ReservedValue<SealedRuntimeDirectoryBundleV1>, RuntimeDirectoryFreezeErrorV1> {
    if !frozen.uses_budget(memory) || !binding.uses_budget(memory) {
        return Err(ValueTreeError::Invalid("sealed directory owner pool").into());
    }
    frozen.validate_sealed(&binding, roots, memory)?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let fee = memory
        .reserve(
            std::mem::size_of::<SealedRuntimeDirectoryBundleV1>() + 128,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let materials =
        frozen
            .materials
            .rebind_after_seal_v1(&binding, roots, memory, work, &mut cancelled)?;
    let inventory_identities = frozen.inventory_identities.rebind_after_seal_v1(
        &binding,
        roots,
        memory,
        identity_work,
        &mut cancelled,
    )?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(fee.retain(SealedRuntimeDirectoryBundleV1 {
        frozen,
        binding,
        materials,
        inventory_identities,
    }))
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
    pub(in crate::world) fn freeze_current_runtime_directories_v1<FR, FC, RR, RC>(
        &mut self,
        caches: &FactoryPhaseAcceptedCachesV1<'_>,
        families: &FactoryPhaseFamiliesV1<'_>,
        factory: &mut FactoryTransitionReadContextV1<'_, FR, FC>,
        recipe: &mut IndustryTransitionReadContextV1<'_, RR, RC>,
        work: &mut FactoryPhaseWorkV1,
        inventory_directory: &ReservedValue<InventoryIdentityDirectoryV1>,
        inventory_family: &ReservedValue<InventoryFamilyQualificationV1>,
        inventory_identity_work: &mut InventoryIdentityDirectoryWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<ReservedValue<FrozenRuntimeDirectoryBundleV1>, RuntimeDirectoryFreezeErrorV1>
    where
        FR: ValueNodeReader,
        FC: FnMut() -> bool,
        RR: ValueNodeReader,
        RC: FnMut() -> bool,
    {
        let result = (|| {
            let live = live_owner();
            let guard = self.ctx.transaction.guard();
            self.ctx.transaction.validate_guard(&guard, &live)?;
            if self.ctx.values.class != MemoryClass::Ordinary
                || !self.ctx.transaction.uses_budget(self.ctx.values.memory)
            {
                return Err(ValueTreeError::Invalid("directory freeze pool/class").into());
            }
            let fee = self
                .ctx
                .values
                .memory
                .reserve(
                    std::mem::size_of::<FrozenRuntimeDirectoryBundleV1>() + 4096,
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            // Establish ALL actual read-set paths first. Later private snapshot guards
            // must agree with the terminal guard; an unexpected new read fails closed.
            let _selected = collect_current_module_invocation_ids_v1(
                caches.invocations,
                self.families.instances,
                self.families.config,
                self.state,
                self.ctx,
                &mut work.enumeration,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            let source = bind_current_agent_orders_v1(
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
            let current_orders = open_current_factory_order_directory_v1(
                caches.orders,
                &source,
                self.state,
                self.ctx,
                &mut work.scan,
                &mut work.typed,
                &mut work.orders,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            let current_records = open_current_factory_recipe_directory_v1(
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
            let guard = self.ctx.transaction.guard();
            let invocations = freeze_current_invocation_directory_v1(
                caches.invocations,
                self.families.instances,
                self.families.config,
                self.state,
                self.ctx,
                &mut work.enumeration,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            let orders = freeze_current_factory_order_directory_v1(
                &current_orders,
                self.ctx,
                &mut work.orders,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            let records = freeze_current_factory_recipe_directory_v1(
                &current_records,
                self.ctx.transaction,
                factory,
                recipe,
                &mut work.factory,
                &mut work.recipe,
                &mut work.records,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            let current_materials = open_current_inventory_material_scope_v1(
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
            let materials = freeze_current_inventory_material_scope_v1(
                current_materials,
                self.ctx,
                &guard,
                &live,
            )?;
            let guard = self.ctx.transaction.guard();
            let inventory_identities = freeze_current_inventory_identity_directory_v1(
                inventory_directory,
                inventory_family,
                self.families.roots,
                self.state,
                self.ctx,
                &mut work.material_family,
                inventory_identity_work,
                &guard,
                &live,
            )?;
            if (self.ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let value = fee.retain(FrozenRuntimeDirectoryBundleV1 {
                guard: self.ctx.transaction.guard(),
                memory: self.ctx.values.memory.clone(),
                orders,
                records,
                invocations,
                materials,
                inventory_identities,
            });
            let terminal_live = live_owner();
            value.validate_terminal_guard(
                self.ctx.transaction,
                self.ctx.values.memory,
                &terminal_live,
            )?;
            if (self.ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            Ok(value)
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
