use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayErrorV1;
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard,
};
#[path = "incremental_factory_lifecycle_index_current_read.rs"]
mod read;
struct FactoryDelta {
    source: ReservedValue<FactoryCurrentOuterChangeV1>,
    after: Option<HeldFactory>,
}
struct JobDelta {
    source: ReservedValue<IndustryCurrentOuterChangeV1>,
    after: Option<HeldJob>,
}
/// One complete Current observation. IDs count every pending recipe job,
/// regardless of phase, and all aggregate facts derive from qualified members.
/// This is data only, never permission to recycle or publish an event.
pub struct FactoryLifecycleObservationV1 {
    factory_id: String,
    jobs: Vec<u64>,
    factory: Option<HeldFactory>,
    completed_without: u128,
    stable_without: bool,
    guard: PreparedGuard,
    memory: MemoryBudget,
}
impl FactoryLifecycleObservationV1 {
    pub fn factory_id(&self) -> &str {
        &self.factory_id
    }
    pub fn active_jobs(&self) -> usize {
        self.jobs.len()
    }
    pub fn recipe_job_ids(&self) -> impl ExactSizeIterator<Item = u64> + '_ {
        self.jobs.iter().copied()
    }
    pub fn factory_exists(&self) -> bool {
        self.factory.is_some()
    }
    pub fn completed_jobs_without_factory(&self) -> Result<u64, ValueTreeError> {
        u64::try_from(self.completed_without)
            .map_err(|_| ValueTreeError::ResourceLimited("factory completed jobs total"))
    }
    /// Mathematical >= 6 stage threshold, independent of diagnostic u64 bounds.
    pub fn stage_completed_jobs_without_factory(&self) -> u64 {
        crate::state::industry_transition::stage_completed_jobs_threshold_v1(self.completed_without)
    }
    pub fn stable_without_factory(&self) -> bool {
        self.stable_without
    }
    pub fn validate<M: MerkleNodeReader>(
        &self,
        tx: &mut ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), WorldFieldOverlayErrorV1> {
        tx.contribute(|tx| {
            tx.validate_guard(&self.guard, live)?;
            if !self.memory.same_pool(memory) || !tx.uses_budget(memory) {
                return Err(invalid().into());
            }
            Ok(())
        })
    }
}
fn factory_position(
    rows: &[HeldFactory],
    id: &str,
    work: &mut FactoryLifecycleIndexWorkV1,
    mut cancel: impl FnMut() -> bool,
) -> Result<Option<usize>, ValueTreeError> {
    let (mut lo, mut hi) = (0, rows.len());
    while lo < hi {
        if cancel() {
            return Err(ValueTreeError::Cancelled);
        }
        let mid = lo + (hi - lo) / 2;
        match work.comparison(&rows[mid].id, id)? {
            std::cmp::Ordering::Less => lo = mid + 1,
            std::cmp::Ordering::Greater => hi = mid,
            std::cmp::Ordering::Equal => return Ok(Some(mid)),
        }
    }
    if cancel() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(None)
}
fn job_position(
    rows: &[HeldJob],
    id: u64,
    work: &mut FactoryLifecycleIndexWorkV1,
    mut cancel: impl FnMut() -> bool,
) -> Result<Option<usize>, ValueTreeError> {
    let (mut lo, mut hi) = (0, rows.len());
    while lo < hi {
        if cancel() {
            return Err(ValueTreeError::Cancelled);
        }
        work.cell()?;
        let mid = lo + (hi - lo) / 2;
        match rows[mid].id.cmp(&id) {
            std::cmp::Ordering::Less => lo = mid + 1,
            std::cmp::Ordering::Greater => hi = mid,
            std::cmp::Ordering::Equal => return Ok(Some(mid)),
        }
    }
    if cancel() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(None)
}
fn range(
    rows: &[HeldJob],
    id: &str,
    upper: bool,
    work: &mut FactoryLifecycleIndexWorkV1,
    mut cancel: impl FnMut() -> bool,
) -> Result<usize, ValueTreeError> {
    let (mut lo, mut hi) = (0, rows.len());
    while lo < hi {
        if cancel() {
            return Err(ValueTreeError::Cancelled);
        }
        let mid = lo + (hi - lo) / 2;
        let cmp = work.comparison(&rows[mid].factory, id)?;
        if cmp.is_lt() || (upper && cmp.is_eq()) {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    if cancel() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(lo)
}
#[allow(clippy::too_many_arguments)]
pub fn observe_current_factory_lifecycle_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    id: &str,
    parent: &ReservedValue<FactoryLifecycleIndexV1>,
    factories: &ReservedValue<FactoryFamilyQualificationV1>,
    industry: &ReservedValue<IndustryFamilyQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    factory_work: &mut FactoryWorkBudgetV1,
    industry_work: &mut IndustryWorkBudgetV1,
    work: &mut FactoryLifecycleIndexWorkV1,
    max_payload_bytes: u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<FactoryLifecycleObservationV1>, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        ctx.check()?;
        let memory = ctx.values.memory;
        let _phase = memory
            .reserve(64 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !parent.uses_budget(memory)
            || !tx.uses_budget(memory)
        {
            return Err(invalid().into());
        }
        parent.validate_binding(factories, industry, roots, memory, MemoryClass::Ordinary)?;
        if parent.roots != tx.parent_fence().parent_roots
            || ctx.root != roots.state_root(StateNamespaceV1::Business)
        {
            return Err(invalid().into());
        }
        if id.len() > work.max_text_bytes {
            return Err(ValueTreeError::ResourceLimited("factory lifecycle query ID").into());
        }
        // This separate continuous cap covers every inspected pending entry,
        // including unchanged reads and entries outside either relevant family.
        // It also rejects family-shaped writes in a foreign namespace before
        // either namespace-specific typed visitor can ignore them.
        let mut pending_remaining = std::mem::take(&mut work.pending_entries_remaining);
        let inspected = (|| {
            let cancel = std::cell::RefCell::new(&mut ctx.cancelled);
            for state in &roots.state {
                tx.visit_pending_writes::<WorldFieldOverlayErrorV1>(
                    state.namespace,
                    &mut pending_remaining,
                    || {
                        if (cancel.borrow_mut())() {
                            Err(ValueTreeError::Cancelled.into())
                        } else {
                            Ok(())
                        }
                    },
                    |raw, _, _| {
                        work.cell()?;
                        if !raw.starts_with(b"oasis7.iws.world-field-key.v1\0") {
                            return Ok(());
                        }
                        let parsed =
                            crate::incremental_world_field_key::WorldFieldKeyV1::from_bytes(raw)
                                .map_err(ValueTreeError::Invalid)?;
                        let descriptor = match parsed.name {
                            "factories" => FactoryRecordKindV1::Factory.descriptor()?,
                            "pending_recipe_jobs" => IndustryRecordKindV1::Recipe.descriptor()?,
                            _ => return Ok(()),
                        };
                        let expected = if parsed.name == "factories" {
                            StateNamespaceV1::Business
                        } else {
                            StateNamespaceV1::Scheduler
                        };
                        let key = crate::incremental_world_field_key::WorldFieldKeyV1::new(
                            descriptor,
                            parsed.kind,
                            parsed.semantic_key,
                        )
                        .map_err(ValueTreeError::Invalid)?;
                        if state.namespace != expected || key.as_bytes() != raw {
                            return Err(invalid().into());
                        }
                        Ok(())
                    },
                )?;
            }
            Ok::<(), WorldFieldOverlayErrorV1>(())
        })();
        work.pending_entries_remaining = pending_remaining;
        inspected?;
        ctx.check()?;
        let bytes = size(
            work.max_pending_records,
            std::mem::size_of::<FactoryDelta>(),
        )?
        .checked_add(size(
            work.max_pending_records,
            std::mem::size_of::<JobDelta>(),
        )?)
        .ok_or_else(invalid)?;
        let _pending = memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut fc = Vec::with_capacity(work.max_pending_records);
        let mut jc = Vec::with_capacity(work.max_pending_records);
        {
            let mut context = FactoryTransitionReadContextV1 {
                values: ValueReadContext {
                    source: ctx.values.source,
                    memory,
                    class: MemoryClass::Ordinary,
                    io: &mut *ctx.values.io,
                },
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                max_payload_bytes,
                cancelled: &mut ctx.cancelled,
            };
            visit_factory_current_outer_changes_v1(
                tx,
                factories,
                &mut context,
                factory_work,
                |change| {
                    work.cell()?;
                    match &*change {
                        FactoryCurrentOuterChangeV1::FormatMarker {
                            before_value_root,
                            after_value_root,
                        } => {
                            if before_value_root != after_value_root {
                                return Err(invalid().into());
                            }
                        }
                        FactoryCurrentOuterChangeV1::Record { .. } => {
                            if fc.len() == fc.capacity() {
                                return Err(ValueTreeError::ResourceLimited(
                                    "factory lifecycle pending records",
                                )
                                .into());
                            }
                            fc.push(FactoryDelta {
                                source: change,
                                after: None,
                            });
                        }
                    }
                    Ok(())
                },
            )?;
        }
        {
            let mut context = IndustryTransitionReadContextV1 {
                values: ValueReadContext {
                    source: ctx.values.source,
                    memory,
                    class: MemoryClass::Ordinary,
                    io: &mut *ctx.values.io,
                },
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                max_payload_bytes,
                cancelled: &mut ctx.cancelled,
            };
            visit_recipe_current_outer_changes_v1(
                tx,
                industry,
                &mut context,
                industry_work,
                |change| {
                    work.cell()?;
                    match &*change {
                        IndustryCurrentOuterChangeV1::FormatMarker {
                            before_value_root,
                            after_value_root,
                        } => {
                            if before_value_root != after_value_root {
                                return Err(invalid().into());
                            }
                        }
                        IndustryCurrentOuterChangeV1::Record { .. } => {
                            if jc.len() == jc.capacity() {
                                return Err(ValueTreeError::ResourceLimited(
                                    "factory lifecycle pending records",
                                )
                                .into());
                            }
                            jc.push(JobDelta {
                                source: change,
                                after: None,
                            });
                        }
                    }
                    Ok(())
                },
            )?;
        }
        let (mut completed, mut stable) = (parent.completed_sum, parent.stable_count);
        let mut factory_count = i128::try_from(parent.factories.len()).map_err(|_| invalid())?;
        let mut job_count = i128::try_from(parent.jobs.len()).map_err(|_| invalid())?;
        for delta in &mut fc {
            ctx.check()?;
            work.cell()?;
            let FactoryCurrentOuterChangeV1::Record { id, before, after } = &*delta.source else {
                return Err(invalid().into());
            };
            let pos = factory_position(&parent.factories, id, work, &mut ctx.cancelled)?;
            let old = pos.map(|i| &parent.factories[i]);
            match (old, before) {
                (Some(old), Some(before))
                    if old.value == before.value_root && old.child == before.record_root => {}
                (None, None) => {}
                _ => return Err(invalid().into()),
            }
            if let Some(after) = after {
                {
                    let mut context = FactoryTransitionReadContextV1 {
                        values: ValueReadContext {
                            source: ctx.values.source,
                            memory,
                            class: MemoryClass::Ordinary,
                            io: &mut *ctx.values.io,
                        },
                        key_work: &mut *ctx.key_work,
                        semantic_bytes: ctx.semantic_bytes,
                        record_bytes: ctx.record_bytes,
                        max_payload_bytes,
                        cancelled: &mut ctx.cancelled,
                    };
                    crate::incremental_factory_records::validate_factory_lifecycle_child_delta_v1(
                        before.as_ref().map(|b| &b.record_root),
                        &after.record_root,
                        &mut context,
                        factory_work,
                    )?;
                }
                delta.after = Some(read::factory(tx, ctx, id, after, work)?);
            }
            factory_count += i128::from(after.is_some()) - i128::from(before.is_some());
            if let Some(old) = old {
                completed = completed
                    .checked_sub(u128::from(old.completed))
                    .ok_or_else(invalid)?;
                stable = stable
                    .checked_sub(u64::from(old.stable))
                    .ok_or_else(invalid)?;
            }
            if let Some(new) = &delta.after {
                completed = completed
                    .checked_add(u128::from(new.completed))
                    .ok_or_else(invalid)?;
                stable = stable
                    .checked_add(u64::from(new.stable))
                    .ok_or_else(invalid)?;
            }
        }
        for delta in &mut jc {
            ctx.check()?;
            work.cell()?;
            let IndustryCurrentOuterChangeV1::Record { id, before, after } = &*delta.source else {
                return Err(invalid().into());
            };
            let pos = job_position(&parent.jobs, *id, work, &mut ctx.cancelled)?;
            let old = pos.map(|i| &parent.jobs[i]);
            match (old, before) {
                (Some(old), Some(before))
                    if old.value == before.value_root && old.child == before.record_root => {}
                (None, None) => {}
                _ => return Err(invalid().into()),
            }
            if let Some(after) = after {
                {
                    let mut context = IndustryTransitionReadContextV1 {
                        values: ValueReadContext {
                            source: ctx.values.source,
                            memory,
                            class: MemoryClass::Ordinary,
                            io: &mut *ctx.values.io,
                        },
                        key_work: &mut *ctx.key_work,
                        semantic_bytes: ctx.semantic_bytes,
                        record_bytes: ctx.record_bytes,
                        max_payload_bytes,
                        cancelled: &mut ctx.cancelled,
                    };
                    crate::incremental_industry_jobs::validate_industry_lifecycle_child_delta_v1(
                        before.as_ref().map(|b| &b.record_root),
                        &after.record_root,
                        &mut context,
                        industry_work,
                    )?;
                }
                delta.after = Some(read::job(tx, ctx, *id, after, work)?);
            }
            job_count += i128::from(after.is_some()) - i128::from(before.is_some());
        }
        if factory_count < 0
            || usize::try_from(factory_count).map_err(|_| invalid())? > work.max_factories
            || job_count < 0
            || usize::try_from(job_count).map_err(|_| invalid())? > work.max_recipe_jobs
        {
            return Err(ValueTreeError::ResourceLimited("factory lifecycle Current counts").into());
        }
        let base = factory_position(&parent.factories, id, work, &mut ctx.cancelled)?;
        let mut target = base.map(|i| parent.factories[i].clone());
        for delta in &fc {
            ctx.check()?;
            let FactoryCurrentOuterChangeV1::Record { id: changed, .. } = &*delta.source else {
                return Err(invalid().into());
            };
            if work.comparison(changed, id)?.is_eq() {
                target = delta.after.clone();
            }
        }
        if let Some(target) = &target {
            // The private cache supplies candidates, never a substitute for
            // actual Current selected data used in the removed-factory total.
            let actual = read::factory(
                tx,
                ctx,
                id,
                &FactoryOuterRecordRootV1 {
                    value_root: target.value,
                    record_root: target.child.clone(),
                },
                work,
            )?;
            if actual.completed != target.completed || actual.stable != target.stable {
                return Err(invalid().into());
            }
            completed = completed
                .checked_sub(u128::from(target.completed))
                .ok_or_else(invalid)?;
            stable = stable
                .checked_sub(u64::from(target.stable))
                .ok_or_else(invalid)?;
        }
        let start = range(&parent.by_factory, id, false, work, &mut ctx.cancelled)?;
        let end = range(&parent.by_factory, id, true, work, &mut ctx.cancelled)?;
        let capacity = (end - start).checked_add(jc.len()).ok_or_else(invalid)?;
        let bytes = std::mem::size_of::<FactoryLifecycleObservationV1>()
            .checked_add(id.len())
            .and_then(|n| n.checked_add(capacity.checked_mul(std::mem::size_of::<u64>())?))
            .ok_or_else(invalid)?;
        let fee = memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut ids = Vec::with_capacity(capacity);
        for job in &parent.by_factory[start..end] {
            ctx.check()?;
            work.cell()?;
            let mut changed = false;
            for delta in &jc {
                work.cell()?;
                let IndustryCurrentOuterChangeV1::Record { id, .. } = &*delta.source else {
                    return Err(invalid().into());
                };
                if *id == job.id {
                    changed = true;
                    break;
                }
            }
            if !changed {
                read::recheck_job(job, id, tx, ctx, work)?;
                ids.push(job.id);
            }
        }
        for delta in &jc {
            ctx.check()?;
            if let Some(job) = &delta.after {
                if work.comparison(&job.factory, id)?.is_eq() {
                    read::recheck_job(job, id, tx, ctx, work)?;
                    ids.push(job.id);
                }
            }
        }
        sort(&mut ids, work, &mut ctx.cancelled, |a, b, w| {
            w.cell()?;
            Ok(a.cmp(b))
        })?;
        ctx.check()?;
        let tail = tx.guard();
        tx.validate_guard(&tail, live)?;
        Ok(fee.retain(FactoryLifecycleObservationV1 {
            factory_id: id.to_owned(),
            jobs: ids,
            factory: target,
            completed_without: completed,
            stable_without: stable > 0,
            guard: tail,
            memory: memory.clone(),
        }))
    })
}
