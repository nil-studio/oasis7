//! Unique recipe progress/stage kernel shared by completion and real action followups.
use super::*;
#[allow(clippy::too_many_arguments)]
pub(in crate::world::incremental_normal_industry_due) fn apply_recipe_progress_v1<
    M,
    P,
    V,
    W,
    D,
    E,
    C,
    CW,
>(
    job: &RecipeJobState,
    job_bound: usize,
    update: RecipeProgressUpdateV1,
    factory: Option<&FactoryState>,
    sources: &FactoryScheduleSourcesV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut FactoryScheduleWorkV1,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    let input = match update {
        RecipeProgressUpdateV1::Completed => RecipeProgressInputV1::Completed {
            job,
            bound: job_bound,
        },
        RecipeProgressUpdateV1::Blocked => RecipeProgressInputV1::Blocked {
            factory_id: &job.factory_id,
            bound: job_bound,
        },
    };
    apply_recipe_progress_input_v1(input, factory, sources, step, read, write, work, live)
}

pub(in crate::world) fn refresh_current_blocked_factory_stage_v1<M, P, V, W, D, E, C, CW>(
    factory_id: &str,
    factory: Option<&FactoryState>,
    sources: &FactoryScheduleSourcesV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut FactoryScheduleWorkV1,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    apply_recipe_progress_input_v1(
        RecipeProgressInputV1::Blocked {
            factory_id,
            bound: 0,
        },
        factory,
        sources,
        step,
        read,
        write,
        work,
        live,
    )
}

#[derive(Clone, Copy)]
enum RecipeProgressInputV1<'a> {
    Completed {
        job: &'a RecipeJobState,
        bound: usize,
    },
    Blocked {
        factory_id: &'a str,
        bound: usize,
    },
}
fn apply_recipe_progress_input_v1<M, P, V, W, D, E, C, CW>(
    input: RecipeProgressInputV1<'_>,
    factory: Option<&FactoryState>,
    sources: &FactoryScheduleSourcesV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut FactoryScheduleWorkV1,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    use oasis7_world_state::{
        incremental_factory_lifecycle_index::observe_current_factory_lifecycle_v1,
        incremental_world_native::{
            increment_current_recipe_progress_v1, recover_current_factory_industry_progress_v1,
            recover_gameplay_policy_overlay_v1,
        },
        state::industry_transition::{
            aggregate_stage_completed_jobs_v1, factory_has_canonical_stable_line,
            industry_stage_for_progress, starter_recipe_completion_milestone_v1,
        },
    };
    let (factory_id, job_bound) = match input {
        RecipeProgressInputV1::Completed { job, bound } => (job.factory_id.as_str(), bound),
        RecipeProgressInputV1::Blocked { factory_id, bound } => (factory_id, bound),
    };
    let now = step.time();
    let mut read = WorldFieldReadContextV1 {
        root: sources
            .roots
            .state_root(oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization),
        state: read.state,
        values: ValueReadContext {
            source: read.values.source,
            memory: read.values.memory,
            class: read.values.class,
            io: &mut *read.values.io,
        },
        key_work: &mut *read.key_work,
        semantic_bytes: read.semantic_bytes,
        record_bytes: read.record_bytes,
        take_state_error: &mut read.take_state_error,
        cancelled: &mut read.cancelled,
    };
    spend(work, factory_id.len())?;
    let policy = recover_gameplay_policy_overlay_v1(
        step.transaction,
        live,
        &mut read,
        &mut work.lifecycle.native_scan,
        &mut work.lifecycle.native_read,
    )
    .map_err(overlay)?;
    let governed =
        policy.value().value().electricity_tax_bps > 0 || policy.value().value().data_tax_bps > 0;
    let guard = step.transaction.guard();
    let progress = recover_current_factory_industry_progress_v1(
        sources.lifecycle.native,
        sources.roots,
        step.transaction,
        &mut read,
        &mut work.lifecycle.native_scan,
        &mut work.lifecycle.native_read,
        &guard,
        live,
    )
    .map_err(overlay)?;
    let value = progress.value().ok_or_else(invalid)?;
    let guard = step.transaction.guard();
    let observation = observe_current_factory_lifecycle_v1(
        factory_id,
        sources.lifecycle.index,
        sources.lifecycle.factories,
        sources.lifecycle.industry,
        sources.roots,
        step.transaction,
        &mut read,
        &mut work.lifecycle.factory,
        &mut work.lifecycle.industry,
        &mut work.lifecycle.lifecycle,
        work.lifecycle.max_scalar_bytes as u64,
        &guard,
        live,
    )
    .map_err(overlay)?;
    observation
        .validate(step.transaction, step.mutation.memory, live)
        .map_err(overlay)?;
    let jobs = aggregate_stage_completed_jobs_v1(
        [
            observation.stage_completed_jobs_without_factory(),
            factory.map_or(0, |f| f.production.completed_jobs),
        ]
        .into_iter(),
    );
    let stable = observation.stable_without_factory()
        || factory.is_some_and(factory_has_canonical_stable_line);
    let stage =
        industry_stage_for_progress(stable, jobs, value.completed_material_transits, governed);
    let layout = job_bound
        .checked_mul(2)
        .and_then(|n| n.checked_add(4096))
        .ok_or_else(invalid)?;
    spend(work, layout)?;
    let _phase = step
        .mutation
        .memory
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    match input {
        RecipeProgressInputV1::Completed { job, .. } => {
            let milestone = if value.starter_industrial_milestone.is_none() {
                starter_recipe_completion_milestone_v1(job, factory.map(|f| &f.output_ledger), now)
            } else {
                None
            };
            increment_current_recipe_progress_v1(
                &progress,
                milestone.as_ref(),
                stage,
                now,
                step.transaction,
                &mut read,
                write,
                &mut work.lifecycle.native_scan,
                &mut work.lifecycle.native_read,
                &mut work.lifecycle.native_write,
                live,
            )
            .map_err(overlay)?;
        }
        RecipeProgressInputV1::Blocked { .. } => {
            oasis7_world_state::incremental_world_native::change_current_factory_industry_stage_v1(
                &progress,
                stage,
                now,
                step.transaction,
                &mut read,
                write,
                &mut work.lifecycle.native_scan,
                &mut work.lifecycle.native_read,
                &mut work.lifecycle.native_write,
                live,
            )
            .map_err(overlay)?;
        }
    }
    Ok(())
}
