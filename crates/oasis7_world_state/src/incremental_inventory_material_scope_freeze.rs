//! True terminal Current material snapshot -> actual sealed after-root cache.
//! Rebinding is structural data only, never durable acceptance/head selection.
use super::*;
use oasis7_formal_protocol::incremental_execution::SealedExecutionBindingV1;
pub struct FrozenInventoryMaterialScopeV1 {
    source: ReservedValue<CurrentInventoryMaterialScopeV1>,
    guard: PreparedGuard,
    memory: MemoryBudget,
}
impl FrozenInventoryMaterialScopeV1 {
    pub fn validate_guard<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory)
            || !self.source.uses_budget(memory)
            || !tx.uses_budget(memory)
        {
            return Err(invalid().into());
        }
        Ok(())
    }

    pub fn validate_sealed(
        &self,
        binding: &SealedExecutionBindingV1,
        roots: &RootSetV1,
        memory: &MemoryBudget,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        if !self.memory.same_pool(memory) || !self.source.uses_budget(memory) {
            return Err(invalid().into());
        }
        binding.validate_source_guard(&self.guard, memory)?;
        binding.validate_after_roots(roots, memory)?;
        Ok(())
    }
    pub fn rebind_after_seal_v1(
        &self,
        binding: &SealedExecutionBindingV1,
        roots: &RootSetV1,
        memory: &MemoryBudget,
        work: &mut InventoryMaterialScopeWorkV1,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<ReservedValue<ParentInventoryMaterialScopeV1>, WorldFieldOverlayErrorV1> {
        self.validate_sealed(binding, roots, memory)?;
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if self.source.legacy.len() > work.max_entries_per_map
            || self.source.world.len() > work.max_entries_per_map
        {
            return Err(invalid().into());
        }
        let fee = memory
            .reserve(admission(work, false)?, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut legacy = Vec::with_capacity(work.max_entries_per_map);
        let mut world = Vec::with_capacity(work.max_entries_per_map);
        if legacy.capacity() > work.max_entries_per_map
            || world.capacity() > work.max_entries_per_map
        {
            return Err(invalid().into());
        }
        copy_rows(&self.source.legacy, &mut legacy, work, &mut cancelled)?;
        copy_rows(&self.source.world, &mut world, work, &mut cancelled)?;
        if cancelled() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(fee.retain(ParentInventoryMaterialScopeV1 {
            roots: binding.after_roots_digest(),
            legacy,
            world,
            world_present: self.source.world_present,
        }))
    }
}
pub fn freeze_current_inventory_material_scope_v1<M, R, E, C>(
    source: ReservedValue<CurrentInventoryMaterialScopeV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<FrozenInventoryMaterialScopeV1>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        source.validate_current(ctx, guard, live)?;
        if !source.uses_budget(ctx.values.memory) {
            return Err(invalid().into());
        }
        let fee = ctx
            .values
            .memory
            .reserve(
                std::mem::size_of::<FrozenInventoryMaterialScopeV1>() + 128,
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        Ok(fee.retain(FrozenInventoryMaterialScopeV1 {
            source,
            guard: ctx.transaction.guard(),
            memory: ctx.values.memory.clone(),
        }))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
