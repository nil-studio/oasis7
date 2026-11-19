//! Direct RecipeStarted effects, using the actual pending/slot/path owners.
use super::super::recipe_current::RecipePathsPlanV1;
use super::*;
use oasis7_world_state::incremental_logistics_records::{
    LogisticsCellChangeV1, LogisticsCellV1, LogisticsColumnV1, LogisticsRecordKeyV1,
    LogisticsRecordKindV1, LogisticsValueRefV1, change_logistics_cell_overlay_v1,
};

struct Production {
    active: u16,
    current_job: Option<u64>,
    current_recipe: ReservedValue<Option<String>>,
    started: Option<u64>,
    status: u64,
}
fn scalar<M, R, E, C>(
    id: &str,
    column: FactoryColumnV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut FactoryScheduleWorkV1,
) -> Result<FactoryValueV1, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    spend(work, id.len())?;
    match read_factory_cell_overlay_v1(
        FactoryRecordKeyV1::Factory(id),
        FactoryCellV1::Column(column),
        ctx,
        8,
        &mut std::io::sink(),
    )
    .map_err(overlay)?
    {
        FactoryCellReadV1::Value(v) => Ok(v),
        _ => Err(invalid()),
    }
}
fn optional(value: FactoryValueV1) -> Result<Option<u64>, BodyMutationError> {
    match value {
        FactoryValueV1::Unit => Ok(None),
        FactoryValueV1::Unsigned(n) => Ok(Some(n)),
        _ => Err(invalid()),
    }
}
fn production<M, R, E, C>(
    id: &str,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut FactoryScheduleWorkV1,
) -> Result<Production, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let active = match scalar(id, FactoryColumnV1::ProductionActiveJobs, ctx, work)? {
        FactoryValueV1::Unsigned(n) => u16::try_from(n).map_err(|_| invalid())?,
        _ => return Err(invalid()),
    };
    let current_job = optional(scalar(
        id,
        FactoryColumnV1::ProductionCurrentJobId,
        ctx,
        work,
    )?)?;
    let started = optional(scalar(
        id,
        FactoryColumnV1::ProductionLastStartedAt,
        ctx,
        work,
    )?)?;
    let status = match scalar(id, FactoryColumnV1::ProductionStatus, ctx, work)? {
        FactoryValueV1::Unsigned(n) if n <= 3 => n,
        _ => return Err(invalid()),
    };
    let cap = work.lifecycle.max_scalar_bytes;
    let fee = ctx
        .values
        .memory
        .reserve(
            cap.checked_add(size_of::<Option<String>>())
                .ok_or_else(limited)?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    let mut bytes = Vec::with_capacity(cap);
    if bytes.capacity() != cap {
        return Err(limited());
    }
    spend(work, id.len())?;
    let value = read_factory_cell_overlay_v1(
        FactoryRecordKeyV1::Factory(id),
        FactoryCellV1::Column(FactoryColumnV1::ProductionCurrentRecipeId),
        ctx,
        cap as u64,
        &mut bytes,
    )
    .map_err(overlay)?;
    let current_recipe = match value {
        FactoryCellReadV1::Value(FactoryValueV1::Unit) if bytes.is_empty() => None,
        FactoryCellReadV1::Value(FactoryValueV1::Text { utf8_bytes })
            if usize::try_from(utf8_bytes).ok() == Some(bytes.len()) =>
        {
            Some(String::from_utf8(bytes).map_err(|_| invalid())?)
        }
        _ => return Err(invalid()),
    };
    Ok(Production {
        active,
        current_job,
        current_recipe: fee.retain(current_recipe),
        started,
        status,
    })
}
fn unsigned_ref(value: Option<u64>) -> FactoryValueRefV1<'static> {
    value.map_or(FactoryValueRefV1::Unit, FactoryValueRefV1::Unsigned)
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world::incremental_normal_action_factory_schedule) fn apply_recipe_v1<
    M,
    P,
    V,
    W,
    D,
    E,
    C,
    CW,
    ME,
>(
    job: &RecipeJobState,
    market_quotes: &[crate::MaterialMarketQuote],
    paths: &RecipePathsPlanV1,
    sources: &FactoryScheduleSourcesV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
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
    ME: FnMut() -> Option<ValueTreeError>,
{
    let now = step.time();
    spend(work, job.factory_id.len())?;
    let before;
    {
        let mut ctx = WorldFieldOverlayContextV1 {
            transaction: &mut *step.transaction,
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
        if !matches!(
            read_industry_cell_overlay_v1(
                IndustryRecordKeyV1 {
                    kind: IndustryRecordKindV1::Recipe,
                    id: job.job_id
                },
                IndustryCellV1::Record,
                &mut ctx,
                0,
                &mut std::io::sink()
            )
            .map_err(overlay)?,
            IndustryCellReadV1::RecordAbsent
        ) {
            return Err(state_error(
                step.mutation.memory,
                "recipe start job is already pending: job_id=".len() + 20,
                format_args!("recipe start job is already pending: job_id={}", job.job_id),
            )?);
        }
        if world_id_set_contains_overlay_v1(
            WorldIdSetFieldV1::SettledRecipeJobs,
            WorldIdSetKeyRefV1::Action(job.job_id),
            &mut ctx,
        )
        .map_err(overlay)?
        {
            return Err(state_error(
                step.mutation.memory,
                "recipe start job identity is already settled: job_id=".len() + 20,
                format_args!(
                    "recipe start job identity is already settled: job_id={}",
                    job.job_id
                ),
            )?);
        }
        if world_id_set_contains_overlay_v1(
            WorldIdSetFieldV1::RetiredFactories,
            WorldIdSetKeyRefV1::Text(&job.factory_id),
            &mut ctx,
        )
        .map_err(overlay)?
        {
            return Err(state_error(
                step.mutation.memory,
                "recipe scheduled for retired factory: factory_id=".len() + job.factory_id.len(),
                format_args!(
                    "recipe scheduled for retired factory: factory_id={}",
                    job.factory_id
                ),
            )?);
        }
        before = production(&job.factory_id, &mut ctx, work)?;
    }
    // The original replay additionally checks its production counter, beyond
    // the complete pending-count business quote used by ScheduleRecipe.
    let guard = step.transaction.guard();
    let selected=oasis7_world_state::incremental_factory_schedule_factory::recover_current_schedule_factory_v1(&job.factory_id,sources.lifecycle.factories,&mut WorldFieldOverlayContextV1{transaction:&mut *step.transaction,values:ValueReadContext{source:read.values.source,memory:read.values.memory,class:read.values.class,io:&mut *read.values.io},key_work:&mut *read.key_work,semantic_bytes:read.semantic_bytes,record_bytes:read.record_bytes,take_state_error:&mut read.take_state_error,cancelled:&mut read.cancelled},&mut work.factory,&guard,live).map_err(overlay)?.ok_or_else(invalid)?;
    let guard = step.transaction.guard();
    let observation=oasis7_world_state::incremental_factory_lifecycle_index::observe_current_factory_lifecycle_v1(&job.factory_id,sources.lifecycle.index,sources.lifecycle.factories,sources.lifecycle.industry,sources.roots,step.transaction,read,&mut work.lifecycle.factory,&mut work.lifecycle.industry,&mut work.lifecycle.lifecycle,work.factory.max_text_bytes as u64,&guard,live).map_err(overlay)?;
    if usize::from(before.active) >= usize::from(selected.slots())
        || observation.active_jobs() >= usize::from(selected.slots())
    {
        let cap="recipe factory has no free execution slot: factory_id= active_jobs= pending_jobs= recipe_slots=".len().checked_add(job.factory_id.len()).and_then(|n|n.checked_add(3*20)).ok_or_else(limited)?;
        return Err(state_error(
            step.mutation.memory,
            cap,
            format_args!(
                "recipe factory has no free execution slot: factory_id={} active_jobs={} pending_jobs={} recipe_slots={}",
                job.factory_id,
                before.active,
                observation.active_jobs(),
                selected.slots()
            ),
        )?);
    }
    fn ledger_bytes(ledger: &MaterialLedgerId) -> usize {
        match ledger {
            MaterialLedgerId::World => 0,
            MaterialLedgerId::Agent(v)
            | MaterialLedgerId::Site(v)
            | MaterialLedgerId::Factory(v) => v.len(),
        }
    }
    let world_output = MaterialLedgerId::World;
    let expected_output = if job.consume_ledger == MaterialLedgerId::World {
        &world_output
    } else if &job.consume_ledger == selected.input() {
        selected.output()
    } else {
        let cap="recipe consume ledger does not match factory input or world fallback: factory_id= consume_ledger= input_ledger=".len().checked_add(job.factory_id.len()).and_then(|n|n.checked_add(ledger_bytes(&job.consume_ledger))).and_then(|n|n.checked_add(ledger_bytes(selected.input()))).and_then(|n|n.checked_add(2*8)).ok_or_else(limited)?;
        return Err(state_error(
            step.mutation.memory,
            cap,
            format_args!(
                "recipe consume ledger does not match factory input or world fallback: factory_id={} consume_ledger={} input_ledger={}",
                job.factory_id,
                job.consume_ledger,
                selected.input()
            ),
        )?);
    };
    if &job.output_ledger != expected_output {
        let cap="recipe output ledger does not match committed factory route: factory_id= output_ledger= expected=".len().checked_add(job.factory_id.len()).and_then(|n|n.checked_add(ledger_bytes(&job.output_ledger))).and_then(|n|n.checked_add(ledger_bytes(expected_output))).and_then(|n|n.checked_add(2*8)).ok_or_else(limited)?;
        return Err(state_error(
            step.mutation.memory,
            cap,
            format_args!(
                "recipe output ledger does not match committed factory route: factory_id={} output_ledger={} expected={}",
                job.factory_id, job.output_ledger, expected_output
            ),
        )?);
    }
    if job.duration_ticks == 0 || job.ready_at <= now {
        return Err(state_error(
            step.mutation.memory,
            "recipe timing commitment is invalid: job_id= duration_ticks= ready_at= now=".len()
                + 4 * 20,
            format_args!(
                "recipe timing commitment is invalid: job_id={} duration_ticks={} ready_at={} now={}",
                job.job_id, job.duration_ticks, job.ready_at, now
            ),
        )?);
    }
    let plan = material_plan::prepare_with_prefixes_v1(
        step,
        materials,
        &job.consume_ledger,
        &job.consume,
        true,
        read,
        &mut work.lifecycle,
        live,
        "recipe consume aggregation failed: ",
        "recipe consume failed: ",
        "recipe consume failed: ",
    )?;
    material_plan::apply(
        &plan,
        &job.consume_ledger,
        step,
        materials,
        read,
        &mut work.lifecycle,
        live,
    )?;
    let owner = job.power_owner_agent_id.as_deref().ok_or_else(invalid)?;
    power(
        owner,
        job.power_required,
        true,
        sources.lifecycle.agents,
        step,
        read,
        work,
        live,
    )?;
    {
        let mut ctx = WorldFieldOverlayContextV1 {
            transaction: &mut *step.transaction,
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
        for (id, amount) in &paths.allocations {
            spend(work, id.len())?;
            let old = paths.path(id).ok_or_else(invalid)?;
            let after = old
                .remaining_recipe_amount
                .checked_sub(*amount)
                .ok_or_else(invalid)?;
            let guard = ctx.transaction.guard();
            change_logistics_cell_overlay_v1(
                LogisticsCellChangeV1 {
                    key: LogisticsRecordKeyV1 {
                        kind: LogisticsRecordKindV1::Path,
                        id,
                    },
                    cell: LogisticsCellV1::Column(LogisticsColumnV1::RemainingRecipeAmount),
                    expected: Some(LogisticsValueRefV1::Signed(old.remaining_recipe_amount)),
                    after: Some(LogisticsValueRefV1::Signed(after)),
                    max_current_payload_bytes: 8,
                },
                &mut ctx,
                write,
                &mut work.logistics_write,
                &guard,
                live,
            )
            .map_err(overlay)?;
        }
        let current_recipe = before
            .current_recipe
            .as_ref()
            .map_or(FactoryValueRefV1::Unit, |s| FactoryValueRefV1::Text(s));
        for (column, expected, after) in [
            (
                FactoryColumnV1::ProductionActiveJobs,
                FactoryValueRefV1::Unsigned(u64::from(before.active)),
                FactoryValueRefV1::Unsigned(u64::from(before.active.saturating_add(1))),
            ),
            (
                FactoryColumnV1::ProductionCurrentJobId,
                unsigned_ref(before.current_job),
                FactoryValueRefV1::Unsigned(job.job_id),
            ),
            (
                FactoryColumnV1::ProductionCurrentRecipeId,
                current_recipe,
                FactoryValueRefV1::Text(&job.recipe_id),
            ),
            (
                FactoryColumnV1::ProductionLastStartedAt,
                unsigned_ref(before.started),
                FactoryValueRefV1::Unsigned(now),
            ),
            (
                FactoryColumnV1::ProductionStatus,
                FactoryValueRefV1::Unsigned(before.status),
                FactoryValueRefV1::Unsigned(1),
            ),
        ] {
            spend(work, job.factory_id.len())?;
            let guard = ctx.transaction.guard();
            change_factory_cell_overlay_v1(
                FactoryCellChangeV1 {
                    key: FactoryRecordKeyV1::Factory(&job.factory_id),
                    cell: FactoryCellV1::Column(column),
                    expected: Some(expected),
                    after: Some(after),
                    max_current_payload_bytes: work.lifecycle.max_scalar_bytes.max(8) as u64,
                },
                &mut ctx,
                write,
                &mut work.lifecycle.factory,
                &guard,
                live,
            )
            .map_err(overlay)?;
        }
    }
    activity::set_recipe_v1(
        owner,
        job.job_id,
        &job.factory_id,
        sources.lifecycle.agents,
        step,
        read,
        work,
        live,
    )?;
    touch(owner, step, read, work)?;
    // Fresh point owner after the preceding writes; retain every unrelated
    // progress field/root and insert only the original quote entries.
    let guard = step.transaction.guard();
    let progress =
        oasis7_world_state::incremental_world_native::recover_current_factory_industry_progress_v1(
            sources.lifecycle.native,
            sources.roots,
            step.transaction,
            read,
            &mut work.lifecycle.native_scan,
            &mut work.lifecycle.native_read,
            &guard,
            live,
        )
        .map_err(overlay)?;
    oasis7_world_state::incremental_world_native::write_current_recipe_market_quotes_v1(
        &progress,
        market_quotes,
        step.transaction,
        read,
        write,
        &mut work.lifecycle.native_scan,
        &mut work.lifecycle.native_read,
        &mut work.lifecycle.native_write,
        live,
    )
    .map_err(overlay)?;
    {
        let mut ctx = WorldFieldOverlayContextV1 {
            transaction: &mut *step.transaction,
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
        let guard = ctx.transaction.guard();
        create_industry_record_overlay_v1(
            IndustryRecordCreateV1 {
                key: IndustryRecordKeyV1 {
                    kind: IndustryRecordKindV1::Recipe,
                    id: job.job_id,
                },
                after: IndustryRecordRefV1::Recipe(job),
                qualification: sources.lifecycle.industry,
            },
            &mut ctx,
            write,
            &mut work.lifecycle.industry,
            &guard,
            live,
        )
        .map_err(overlay)?;
    }
    Ok(())
}
