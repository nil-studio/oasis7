//! Complete Factory/Recipe candidate directories. No business qualification.
//! GEN/cold scans authentic parents; ordinary mode consumes complete real deltas.
use crate::world::{
    incremental_factory_directory_orders::FactoryOrderDirectoryErrorV1 as Error,
    incremental_world_event_counter::WorldEventCounterV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::VerifiedFormalCommitV1,
    incremental_execution::{
        ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
    },
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_factory_records::{
        FactoryCurrentOuterChangeV1, FactoryFamilyQualificationV1, FactoryFamilyReadContextV1,
        FactoryQualifiedOuterIdentityV1, FactoryTransitionReadContextV1, FactoryWorkBudgetV1,
        visit_factory_current_outer_changes_v1, visit_qualified_factory_parent_identities_v1,
    },
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_industry_jobs::{
        IndustryCurrentOuterChangeV1, IndustryFamilyQualificationV1, IndustryFamilyReadContextV1,
        IndustryQualifiedOuterIdentityV1, IndustryTransitionReadContextV1, IndustryWorkBudgetV1,
        visit_qualified_recipe_parent_identities_v1, visit_recipe_current_outer_changes_v1,
    },
    incremental_runtime_history_source::AcceptedRuntimeHistorySourceV1,
};

pub struct FactoryRecipeDirectoryWorkV1 {
    pub max_factories: usize,
    pub max_recipe_jobs: usize,
    pub max_factory_id_bytes: usize,
    pub max_outer_changes: usize,
    pub comparisons_remaining: u64,
    pub pending_entries_remaining: usize,
}
pub struct AcceptedFactoryRecipeDirectoryV1 {
    commit: [u8; 32],
    roots: [u8; 32],
    factories: Vec<String>,
    recipes: Vec<u64>,
}
struct FactoryDelta {
    id: Option<String>,
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
}
struct RecipeDelta {
    id: Option<u64>,
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
}
impl FactoryDelta {
    fn matches(&self, value: &FactoryCurrentOuterChangeV1) -> bool {
        match value {
            FactoryCurrentOuterChangeV1::FormatMarker {
                before_value_root,
                after_value_root,
            } => {
                self.id.is_none()
                    && self.before == Some(*before_value_root)
                    && self.after == Some(*after_value_root)
            }
            FactoryCurrentOuterChangeV1::Record { id, before, after } => {
                self.id.as_deref() == Some(id.as_str())
                    && self.before == before.as_ref().map(|v| v.value_root)
                    && self.after == after.as_ref().map(|v| v.value_root)
            }
        }
    }
}
impl RecipeDelta {
    fn matches(&self, value: &IndustryCurrentOuterChangeV1) -> bool {
        match value {
            IndustryCurrentOuterChangeV1::FormatMarker {
                before_value_root,
                after_value_root,
            } => {
                self.id.is_none()
                    && self.before == Some(*before_value_root)
                    && self.after == Some(*after_value_root)
            }
            IndustryCurrentOuterChangeV1::Record { id, before, after } => {
                self.id == Some(*id)
                    && self.before == before.as_ref().map(|v| v.value_root)
                    && self.after == after.as_ref().map(|v| v.value_root)
            }
        }
    }
}
/// All IDs are immutable and privately constructed after both visits finish.
pub struct CurrentFactoryRecipeDirectoryV1<'q> {
    commit: [u8; 32],
    roots: [u8; 32],
    identity: PreparedGuard,
    memory: MemoryBudget,
    factory_q: &'q ReservedValue<FactoryFamilyQualificationV1>,
    recipe_q: &'q ReservedValue<IndustryFamilyQualificationV1>,
    factories: Vec<String>,
    recipes: Vec<u64>,
    factory_delta: Vec<FactoryDelta>,
    recipe_delta: Vec<RecipeDelta>,
}
impl CurrentFactoryRecipeDirectoryV1<'_> {
    pub fn iter_factory_ids(&self) -> impl ExactSizeIterator<Item = &str> {
        self.factories.iter().map(String::as_str)
    }
    pub fn iter_recipe_job_ids(&self) -> impl ExactSizeIterator<Item = u64> + '_ {
        self.recipes.iter().copied()
    }
    #[allow(clippy::too_many_arguments)]
    pub fn validate_current<M, R, C, V, D>(
        &self,
        tx: &mut ExecutionTransaction<'_, M>,
        factory: &mut FactoryTransitionReadContextV1<'_, R, C>,
        recipe: &mut IndustryTransitionReadContextV1<'_, V, D>,
        factory_work: &mut FactoryWorkBudgetV1,
        recipe_work: &mut IndustryWorkBudgetV1,
        work: &mut FactoryRecipeDirectoryWorkV1,
        guard: &PreparedGuard,
        live: &ExecutionFence,
    ) -> Result<(), Error>
    where
        M: MerkleNodeReader,
        R: ValueNodeReader,
        C: FnMut() -> bool,
        V: ValueNodeReader,
        D: FnMut() -> bool,
    {
        let result = (|| {
            tx.validate_guard(guard, live)?;
            tx.validate_transaction_identity(&self.identity)?;
            let fence = tx.parent_fence();
            if self.commit != fence.parent_commit
                || self.roots != fence.parent_roots
                || !self.memory.same_pool(factory.values.memory)
                || !self.memory.same_pool(recipe.values.memory)
                || !tx.uses_budget(&self.memory)
                || factory.values.class != MemoryClass::Ordinary
                || recipe.values.class != MemoryClass::Ordinary
            {
                return Err(
                    ValueTreeError::Invalid("Current Factory/Recipe directory binding").into(),
                );
            }
            let mut index = 0;
            visit_factory_current_outer_changes_v1(
                tx,
                self.factory_q,
                factory,
                factory_work,
                |value| {
                    let cost = self
                        .factory_delta
                        .get(index)
                        .and_then(|v| v.id.as_ref())
                        .map_or(128, |id| id.len().saturating_add(128));
                    spend(&mut work.comparisons_remaining, cost)
                        .map_err(WorldFieldOverlayErrorV1::Value)?;
                    if !self
                        .factory_delta
                        .get(index)
                        .is_some_and(|expected| expected.matches(&value))
                    {
                        return Err(ValueTreeError::Invalid(
                            "Factory directory Current changes changed",
                        )
                        .into());
                    }
                    index += 1;
                    Ok(())
                },
            )?;
            if index != self.factory_delta.len() {
                return Err(
                    ValueTreeError::Invalid("Factory directory missing Current changes").into(),
                );
            }
            let mut index = 0;
            visit_recipe_current_outer_changes_v1(
                tx,
                self.recipe_q,
                recipe,
                recipe_work,
                |value| {
                    spend(&mut work.comparisons_remaining, 128)
                        .map_err(WorldFieldOverlayErrorV1::Value)?;
                    if !self
                        .recipe_delta
                        .get(index)
                        .is_some_and(|expected| expected.matches(&value))
                    {
                        return Err(ValueTreeError::Invalid(
                            "Recipe directory Current changes changed",
                        )
                        .into());
                    }
                    index += 1;
                    Ok(())
                },
            )?;
            if index != self.recipe_delta.len() {
                return Err(
                    ValueTreeError::Invalid("Recipe directory missing Current changes").into(),
                );
            }
            Ok(())
        })();
        if result.is_err() {
            let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
        }
        result
    }
}
fn spend(left: &mut u64, n: usize) -> Result<(), ValueTreeError> {
    *left = left
        .checked_sub(
            u64::try_from(n)
                .map_err(|_| ValueTreeError::ResourceLimited("directory comparison bound"))?,
        )
        .ok_or(ValueTreeError::ResourceLimited("directory comparison work"))?;
    Ok(())
}
fn size(work: &FactoryRecipeDirectoryWorkV1, current: bool) -> Result<usize, ValueTreeError> {
    let extra = if current { work.max_outer_changes } else { 0 };
    work.max_factories
        .checked_add(extra)
        .and_then(|n| {
            n.checked_mul(std::mem::size_of::<String>().checked_add(work.max_factory_id_bytes)?)
        })
        .and_then(|n| {
            n.checked_add(
                work.max_recipe_jobs
                    .checked_add(extra)?
                    .checked_mul(std::mem::size_of::<u64>())?,
            )
        })
        .and_then(|n| {
            n.checked_add(
                extra.checked_mul(
                    std::mem::size_of::<FactoryDelta>()
                        .checked_add(work.max_factory_id_bytes)?
                        .checked_add(std::mem::size_of::<RecipeDelta>())?,
                )?,
            )
        })
        .and_then(|n| n.checked_add(4096))
        .ok_or(ValueTreeError::ResourceLimited(
            "Factory/Recipe directory heap bound",
        ))
}
fn sort_work(work: &mut FactoryRecipeDirectoryWorkV1, current: bool) -> Result<(), ValueTreeError> {
    let extra = if current { work.max_outer_changes } else { 0 };
    let f = work
        .max_factories
        .checked_add(extra)
        .ok_or(ValueTreeError::ResourceLimited("directory sort bound"))?;
    let r = work
        .max_recipe_jobs
        .checked_add(extra)
        .ok_or(ValueTreeError::ResourceLimited("directory sort bound"))?;
    let cost = f
        .checked_mul(f)
        .and_then(|n| n.checked_add(r.checked_mul(r)?))
        .and_then(|n| n.checked_add(f.checked_mul(extra)?))
        .and_then(|n| n.checked_add(r.checked_mul(extra)?))
        .and_then(|n| n.checked_mul(work.max_factory_id_bytes.max(8)))
        .and_then(|n| n.checked_mul(8))
        .ok_or(ValueTreeError::ResourceLimited("directory sort bound"))?;
    spend(&mut work.comparisons_remaining, cost)
}

#[allow(clippy::too_many_arguments)]
pub fn build_accepted_factory_recipe_directory_v1<M, P, R, E, C, Q, V, ER, D>(
    parent: &ReservedValue<AcceptedRuntimeHistorySourceV1>,
    head: &VerifiedFormalCommitV1,
    roots: &RootSetV1,
    factory_q: &ReservedValue<FactoryFamilyQualificationV1>,
    recipe_q: &ReservedValue<IndustryFamilyQualificationV1>,
    tx: &mut ExecutionTransaction<'_, M>,
    factory: &mut FactoryFamilyReadContextV1<'_, P, R, E, C>,
    recipe: &mut IndustryFamilyReadContextV1<'_, Q, V, ER, D>,
    factory_work: &mut FactoryWorkBudgetV1,
    recipe_work: &mut IndustryWorkBudgetV1,
    work: &mut FactoryRecipeDirectoryWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<AcceptedFactoryRecipeDirectoryV1>, Error>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    Q: MerkleNodeReader,
    V: ValueNodeReader,
    ER: FnMut() -> Option<ValueTreeError>,
    D: FnMut() -> bool,
{
    let result = (|| {
        tx.validate_guard(guard, live)?;
        let memory = factory.values.memory;
        if factory.values.class != MemoryClass::Ordinary
            || recipe.values.class != MemoryClass::Ordinary
            || !memory.same_pool(recipe.values.memory)
            || !tx.uses_budget(memory)
        {
            return Err(
                ValueTreeError::Invalid("Factory/Recipe parent directory pool/class").into(),
            );
        }
        let fee = memory
            .reserve(size(work, false)?, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let _accepted = WorldEventCounterV1::begin(parent, head, tx, memory)?;
        if roots
            .digest()
            .map_err(|_| ValueTreeError::Invalid("directory root set"))?
            != tx.parent_fence().parent_roots
        {
            return Err(ValueTreeError::Invalid("directory parent roots").into());
        }
        let mut changed = false;
        for namespace in [
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Allocator,
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Scheduler,
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization,
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Effects,
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Artifacts,
        ] {
            tx.visit_pending_writes(
                namespace,
                &mut work.pending_entries_remaining,
                || {
                    if (factory.cancelled)() || (recipe.cancelled)() {
                        Err(ExecutionError::Aborted)
                    } else {
                        Ok(())
                    }
                },
                |_, _, _| {
                    changed = true;
                    Ok::<(), ExecutionError>(())
                },
            )?;
        }
        if changed {
            return Err(ValueTreeError::Invalid(
                "Factory/Recipe parent directory requires pristine transaction",
            )
            .into());
        }
        sort_work(work, false)?;
        let mut factories = Vec::with_capacity(work.max_factories);
        let mut recipes = Vec::with_capacity(work.max_recipe_jobs);
        if factories.capacity() > work.max_factories || recipes.capacity() > work.max_recipe_jobs {
            return Err(ValueTreeError::ResourceLimited("directory actual capacity").into());
        }
        visit_qualified_factory_parent_identities_v1(
            roots,
            factory_q,
            factory,
            factory_work,
            |value| {
                if let FactoryQualifiedOuterIdentityV1::Record { id, .. } = &*value {
                    if factories.len() == work.max_factories || id.len() > work.max_factory_id_bytes
                    {
                        return Err(ValueTreeError::ResourceLimited(
                            "directory parent factories",
                        ));
                    }
                    let copy = id.to_owned();
                    if copy.capacity() > work.max_factory_id_bytes {
                        return Err(ValueTreeError::ResourceLimited(
                            "directory factory ID capacity",
                        ));
                    }
                    factories.push(copy);
                }
                Ok(())
            },
        )?;
        visit_qualified_recipe_parent_identities_v1(
            roots,
            recipe_q,
            recipe,
            recipe_work,
            |value| {
                if let IndustryQualifiedOuterIdentityV1::Record { id, .. } = &*value {
                    if recipes.len() == work.max_recipe_jobs {
                        return Err(ValueTreeError::ResourceLimited("directory parent recipes"));
                    }
                    recipes.push(*id);
                }
                Ok(())
            },
        )?;
        if (factory.cancelled)() || (recipe.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        factories.sort_unstable();
        recipes.sort_unstable();
        if factories.windows(2).any(|v| v[0] == v[1]) || recipes.windows(2).any(|v| v[0] == v[1]) {
            return Err(ValueTreeError::Invalid("directory duplicate parent IDs").into());
        }
        if (factory.cancelled)() || (recipe.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let fence = tx.parent_fence();
        Ok(fee.retain(AcceptedFactoryRecipeDirectoryV1 {
            commit: fence.parent_commit,
            roots: fence.parent_roots,
            factories,
            recipes,
        }))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}

#[allow(clippy::too_many_arguments)]
pub fn open_current_factory_recipe_directory_v1<'q, M, R, C, V, D>(
    parent: &ReservedValue<AcceptedFactoryRecipeDirectoryV1>,
    factory_q: &'q ReservedValue<FactoryFamilyQualificationV1>,
    recipe_q: &'q ReservedValue<IndustryFamilyQualificationV1>,
    tx: &mut ExecutionTransaction<'_, M>,
    factory: &mut FactoryTransitionReadContextV1<'_, R, C>,
    recipe: &mut IndustryTransitionReadContextV1<'_, V, D>,
    factory_work: &mut FactoryWorkBudgetV1,
    recipe_work: &mut IndustryWorkBudgetV1,
    work: &mut FactoryRecipeDirectoryWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<CurrentFactoryRecipeDirectoryV1<'q>>, Error>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    C: FnMut() -> bool,
    V: ValueNodeReader,
    D: FnMut() -> bool,
{
    let result = (|| {
        tx.validate_guard(guard, live)?;
        let memory = factory.values.memory;
        let fence = tx.parent_fence();
        if factory.values.class != MemoryClass::Ordinary
            || recipe.values.class != MemoryClass::Ordinary
            || !memory.same_pool(recipe.values.memory)
            || !tx.uses_budget(memory)
            || !parent.uses_budget(memory)
            || parent.commit != fence.parent_commit
            || parent.roots != fence.parent_roots
        {
            return Err(ValueTreeError::Invalid("Factory/Recipe Current directory binding").into());
        }
        let fee = memory
            .reserve(size(work, true)?, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        sort_work(work, true)?;
        let mut factory_delta = Vec::with_capacity(work.max_outer_changes);
        let mut recipe_delta = Vec::with_capacity(work.max_outer_changes);
        if factory_delta.capacity() > work.max_outer_changes
            || recipe_delta.capacity() > work.max_outer_changes
        {
            return Err(ValueTreeError::ResourceLimited("directory delta actual capacity").into());
        }
        visit_factory_current_outer_changes_v1(tx, factory_q, factory, factory_work, |value| {
            if factory_delta.len() == work.max_outer_changes {
                return Err(ValueTreeError::ResourceLimited("directory factory changes").into());
            }
            factory_delta.push(match &*value {
                FactoryCurrentOuterChangeV1::FormatMarker {
                    before_value_root,
                    after_value_root,
                } => FactoryDelta {
                    id: None,
                    before: Some(*before_value_root),
                    after: Some(*after_value_root),
                },
                FactoryCurrentOuterChangeV1::Record { id, before, after } => {
                    if id.len() > work.max_factory_id_bytes {
                        return Err(
                            ValueTreeError::ResourceLimited("directory factory ID bytes").into(),
                        );
                    }
                    let copy = id.to_owned();
                    if copy.capacity() > work.max_factory_id_bytes {
                        return Err(ValueTreeError::ResourceLimited(
                            "directory factory ID capacity",
                        )
                        .into());
                    }
                    FactoryDelta {
                        id: Some(copy),
                        before: before.as_ref().map(|v| v.value_root),
                        after: after.as_ref().map(|v| v.value_root),
                    }
                }
            });
            Ok(())
        })?;
        visit_recipe_current_outer_changes_v1(tx, recipe_q, recipe, recipe_work, |value| {
            if recipe_delta.len() == work.max_outer_changes {
                return Err(ValueTreeError::ResourceLimited("directory recipe changes").into());
            }
            recipe_delta.push(match &*value {
                IndustryCurrentOuterChangeV1::FormatMarker {
                    before_value_root,
                    after_value_root,
                } => RecipeDelta {
                    id: None,
                    before: Some(*before_value_root),
                    after: Some(*after_value_root),
                },
                IndustryCurrentOuterChangeV1::Record { id, before, after } => RecipeDelta {
                    id: Some(*id),
                    before: before.as_ref().map(|v| v.value_root),
                    after: after.as_ref().map(|v| v.value_root),
                },
            });
            Ok(())
        })?;
        // Only after BOTH provisional visitors return complete success may
        // their data be folded into the authoritative selection snapshot.
        let fcap = work
            .max_factories
            .checked_add(work.max_outer_changes)
            .ok_or(ValueTreeError::ResourceLimited(
                "directory factory capacity",
            ))?;
        let rcap = work
            .max_recipe_jobs
            .checked_add(work.max_outer_changes)
            .ok_or(ValueTreeError::ResourceLimited("directory recipe capacity"))?;
        let mut factories = Vec::with_capacity(fcap);
        let mut recipes = Vec::with_capacity(rcap);
        if factories.capacity() > fcap || recipes.capacity() > rcap {
            return Err(ValueTreeError::ResourceLimited("directory merged actual capacity").into());
        }
        for id in &parent.factories {
            if (factory.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if id.len() > work.max_factory_id_bytes || factories.len() == fcap {
                return Err(ValueTreeError::ResourceLimited("directory factory merge").into());
            }
            let copy = id.to_owned();
            if copy.capacity() > work.max_factory_id_bytes {
                return Err(
                    ValueTreeError::ResourceLimited("directory factory copy capacity").into(),
                );
            }
            factories.push(copy);
        }
        for entry in &factory_delta {
            if (factory.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if let Some(id) = &entry.id {
                if entry.after.is_some() {
                    if factories.len() == fcap {
                        return Err(
                            ValueTreeError::ResourceLimited("directory factory merge").into()
                        );
                    }
                    let copy = id.to_owned();
                    if copy.capacity() > work.max_factory_id_bytes {
                        return Err(ValueTreeError::ResourceLimited(
                            "directory factory copy capacity",
                        )
                        .into());
                    }
                    factories.push(copy);
                }
            }
        }
        for id in &parent.recipes {
            if (recipe.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if recipes.len() == rcap {
                return Err(ValueTreeError::ResourceLimited("directory recipe merge").into());
            }
            recipes.push(*id);
        }
        for entry in &recipe_delta {
            if (recipe.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if let Some(id) = entry.id {
                if entry.after.is_some() {
                    if recipes.len() == rcap {
                        return Err(ValueTreeError::ResourceLimited("directory recipe merge").into());
                    }
                    recipes.push(id);
                }
            }
        }
        factories.sort_unstable();
        factories.dedup();
        recipes.sort_unstable();
        recipes.dedup();
        let mut kept = 0;
        for index in 0..factories.len() {
            if (factory.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if !factory_delta
                .iter()
                .any(|d| d.id.as_deref() == Some(factories[index].as_str()) && d.after.is_none())
            {
                factories.swap(kept, index);
                kept += 1;
            }
        }
        factories.truncate(kept);
        let mut kept = 0;
        for index in 0..recipes.len() {
            if (recipe.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if !recipe_delta
                .iter()
                .any(|d| d.id == Some(recipes[index]) && d.after.is_none())
            {
                recipes.swap(kept, index);
                kept += 1;
            }
        }
        recipes.truncate(kept);
        if factories.len() > work.max_factories || recipes.len() > work.max_recipe_jobs {
            return Err(ValueTreeError::ResourceLimited("directory current ID count").into());
        }
        if (factory.cancelled)() || (recipe.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(fee.retain(CurrentFactoryRecipeDirectoryV1 {
            commit: fence.parent_commit,
            roots: fence.parent_roots,
            identity: tx.guard(),
            memory: memory.clone(),
            factory_q,
            recipe_q,
            factories,
            recipes,
            factory_delta,
            recipe_delta,
        }))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
