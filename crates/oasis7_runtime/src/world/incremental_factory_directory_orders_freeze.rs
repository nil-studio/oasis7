//! Typed held-fee Current order snapshot, never accepted-head authority.
use super::*;
use oasis7_formal_protocol::incremental_execution::{
    ExecutionTransaction, RootSetV1, SealedExecutionBindingV1,
};
pub(in crate::world) struct FrozenFactoryOrderDirectoryV1 {
    guard: PreparedGuard,
    memory: MemoryBudget,
    ids: Vec<String>,
    indices: Vec<Arc<CachedOrderIndexV1>>,
}
impl FrozenFactoryOrderDirectoryV1 {
    pub(in crate::world) fn validate_guard<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), FactoryOrderDirectoryErrorV1> {
        tx.validate_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory) || !tx.uses_budget(memory) {
            return Err(ValueTreeError::Invalid("frozen order pool").into());
        }
        Ok(())
    }
    pub(in crate::world) fn validate_sealed(
        &self,
        binding: &SealedExecutionBindingV1,
        roots: &RootSetV1,
        memory: &MemoryBudget,
    ) -> Result<(), FactoryOrderDirectoryErrorV1> {
        if !self.memory.same_pool(memory) {
            return Err(ValueTreeError::Invalid("frozen order pool").into());
        }
        binding.validate_source_guard(&self.guard, memory)?;
        binding.validate_after_roots(roots, memory)?;
        Ok(())
    }
    pub(in crate::world) fn iter_order_indices(
        &self,
    ) -> impl ExactSizeIterator<Item = (&str, &CurrentAgentOrderIndexV1)> {
        self.ids
            .iter()
            .zip(&self.indices)
            .map(|(id, index)| (id.as_str(), &*index.index))
    }
}
pub(in crate::world) fn freeze_current_factory_order_directory_v1<M, R, E, C>(
    source: &ReservedValue<CurrentFactoryOrderDirectoryV1<'_>>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut FactoryOrderDirectoryWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<FrozenFactoryOrderDirectoryV1>, FactoryOrderDirectoryErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        source.validate_current(ctx, guard, live)?;
        if !source.uses_budget(ctx.values.memory)
            || source.ids.len() > work.ids.max_ids
            || source.indices.len() != source.ids.len()
        {
            return Err(ValueTreeError::Invalid("freeze order source").into());
        }
        let mut bytes = 4096usize;
        for id in &source.ids {
            if (ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if id.len() > work.ids.max_id_bytes {
                return Err(ValueTreeError::ResourceLimited("freeze order id bytes").into());
            }
            bytes = bytes
                .checked_add(id.len())
                .and_then(|v| {
                    v.checked_add(
                        std::mem::size_of::<String>()
                            + std::mem::size_of::<Arc<CachedOrderIndexV1>>(),
                    )
                })
                .ok_or(ValueTreeError::ResourceLimited("freeze order heap"))?;
            work.merge_comparison_bytes_remaining = work
                .merge_comparison_bytes_remaining
                .checked_sub(id.len() as u64)
                .ok_or(ValueTreeError::ResourceLimited("freeze order byte work"))?;
        }
        let fee = ctx
            .values
            .memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut ids = Vec::with_capacity(source.ids.len());
        let mut indices = Vec::with_capacity(source.indices.len());
        if ids.capacity() > source.ids.len() || indices.capacity() > source.indices.len() {
            return Err(ValueTreeError::ResourceLimited("freeze order actual capacity").into());
        }
        for (id, index) in source.ids.iter().zip(&source.indices) {
            if (ctx.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let copy = id.clone();
            if copy.capacity() > id.len() {
                return Err(ValueTreeError::ResourceLimited("freeze order string capacity").into());
            }
            ids.push(copy);
            indices.push(Arc::clone(index));
        }
        Ok(fee.retain(FrozenFactoryOrderDirectoryV1 {
            guard: ctx.transaction.guard(),
            memory: ctx.values.memory.clone(),
            ids,
            indices,
        }))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
