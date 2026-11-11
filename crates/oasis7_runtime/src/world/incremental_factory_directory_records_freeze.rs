//! Complete typed final Current identities retained independently of borrowed Qs.
use super::*;
use oasis7_formal_protocol::incremental_execution::SealedExecutionBindingV1;
pub(in crate::world) struct FrozenFactoryRecipeDirectoryV1 {
    guard: PreparedGuard,
    memory: MemoryBudget,
    factories: Vec<String>,
    recipes: Vec<u64>,
}
impl FrozenFactoryRecipeDirectoryV1 {
    pub(in crate::world) fn validate_guard<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), Error> {
        tx.validate_guard(&self.guard, live)?;
        if !self.memory.same_pool(memory) || !tx.uses_budget(memory) {
            return Err(ValueTreeError::Invalid("frozen Factory/Recipe pool").into());
        }
        Ok(())
    }
    pub(in crate::world) fn validate_sealed(
        &self,
        binding: &SealedExecutionBindingV1,
        roots: &RootSetV1,
        memory: &MemoryBudget,
    ) -> Result<(), Error> {
        if !self.memory.same_pool(memory) {
            return Err(ValueTreeError::Invalid("frozen Factory/Recipe pool").into());
        }
        binding.validate_source_guard(&self.guard, memory)?;
        binding.validate_after_roots(roots, memory)?;
        Ok(())
    }
    pub(in crate::world) fn iter_factory_ids(&self) -> impl ExactSizeIterator<Item = &str> {
        self.factories.iter().map(String::as_str)
    }
    pub(in crate::world) fn iter_recipe_job_ids(&self) -> impl ExactSizeIterator<Item = u64> + '_ {
        self.recipes.iter().copied()
    }
}
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn freeze_current_factory_recipe_directory_v1<M, R, C, V, D>(
    source: &ReservedValue<CurrentFactoryRecipeDirectoryV1<'_>>,
    tx: &mut ExecutionTransaction<'_, M>,
    factory: &mut FactoryTransitionReadContextV1<'_, R, C>,
    recipe: &mut IndustryTransitionReadContextV1<'_, V, D>,
    factory_work: &mut FactoryWorkBudgetV1,
    recipe_work: &mut IndustryWorkBudgetV1,
    work: &mut FactoryRecipeDirectoryWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<FrozenFactoryRecipeDirectoryV1>, Error>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    C: FnMut() -> bool,
    V: ValueNodeReader,
    D: FnMut() -> bool,
{
    let result = (|| {
        source.validate_current(
            tx,
            factory,
            recipe,
            factory_work,
            recipe_work,
            work,
            guard,
            live,
        )?;
        if !source.uses_budget(factory.values.memory)
            || source.factories.len() > work.max_factories
            || source.recipes.len() > work.max_recipe_jobs
        {
            return Err(ValueTreeError::Invalid("freeze Factory/Recipe source").into());
        }
        let mut bytes = source
            .recipes
            .len()
            .checked_mul(std::mem::size_of::<u64>())
            .and_then(|n| n.checked_add(4096))
            .ok_or(ValueTreeError::ResourceLimited(
                "freeze Factory/Recipe heap",
            ))?;
        for id in &source.factories {
            if (factory.cancelled)() || (recipe.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if id.len() > work.max_factory_id_bytes {
                return Err(ValueTreeError::ResourceLimited("freeze Factory id bytes").into());
            }
            bytes = bytes
                .checked_add(std::mem::size_of::<String>())
                .and_then(|n| n.checked_add(id.len()))
                .ok_or(ValueTreeError::ResourceLimited(
                    "freeze Factory/Recipe heap",
                ))?;
            spend(&mut work.comparisons_remaining, id.len())?;
        }
        spend(
            &mut work.comparisons_remaining,
            source
                .recipes
                .len()
                .checked_mul(8)
                .ok_or(ValueTreeError::ResourceLimited("freeze Recipe byte work"))?,
        )?;
        let fee = factory
            .values
            .memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut factories = Vec::with_capacity(source.factories.len());
        let mut recipes = Vec::with_capacity(source.recipes.len());
        if factories.capacity() > source.factories.len()
            || recipes.capacity() > source.recipes.len()
        {
            return Err(
                ValueTreeError::ResourceLimited("freeze Factory/Recipe actual capacity").into(),
            );
        }
        for id in &source.factories {
            if (factory.cancelled)() || (recipe.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            let copy = id.clone();
            if copy.capacity() > id.len() {
                return Err(ValueTreeError::ResourceLimited(
                    "freeze Factory actual string capacity",
                )
                .into());
            }
            factories.push(copy);
        }
        for id in &source.recipes {
            if (factory.cancelled)() || (recipe.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            recipes.push(*id);
        }
        Ok(fee.retain(FrozenFactoryRecipeDirectoryV1 {
            guard: tx.guard(),
            memory: factory.values.memory.clone(),
            factories,
            recipes,
        }))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
