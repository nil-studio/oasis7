//! RecipeCompleted Current writes. Every helper is state-only; the completed
//! publication owner is created only after the whole terminal commit succeeds.
use super::*;
use crate::world::{
    incremental_body::BodyMutationError,
    incremental_normal_action_factory_schedule::{FactoryScheduleSourcesV1, FactoryScheduleWorkV1},
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::{
    FactoryState, RecipeJobState,
    incremental_factory_records::{
        FactoryRecordChangeV1, FactoryRecordKeyV1, FactoryRecordRefV1,
        replace_factory_record_overlay_v1,
    },
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
};

fn invalid() -> BodyMutationError {
    BodyMutationError::Invalid("recipe terminal Current commit")
}
pub(super) fn overlay(error: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match error {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        WorldFieldOverlayErrorV1::Member(e) => BodyMutationError::Original(Box::new(e)),
    }
}
#[derive(Debug)]
struct StateCause {
    cause: crate::WorldError,
    _fee: MemoryReservation,
}
impl std::fmt::Display for StateCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.cause)
    }
}
impl std::error::Error for StateCause {}
pub(super) fn state_error(
    memory: &MemoryBudget,
    max: usize,
    args: std::fmt::Arguments<'_>,
) -> BodyMutationError {
    use std::fmt::Write;
    let Some(layout) = max
        .checked_mul(2)
        .and_then(|n| n.checked_add(std::mem::size_of::<StateCause>()))
    else {
        return invalid();
    };
    let fee = match memory.reserve(layout, MemoryClass::Ordinary) {
        Ok(f) => f,
        Err(e) => return ValueTreeError::Memory(e).into(),
    };
    let mut reason = String::with_capacity(max);
    if reason.write_fmt(args).is_err() || reason.len() > max {
        return invalid();
    }
    BodyMutationError::Original(Box::new(StateCause {
        cause: crate::WorldError::ResourceBalanceInvalid { reason },
        _fee: fee,
    }))
}
pub(super) fn spend(
    work: &mut FactoryScheduleWorkV1,
    bytes: usize,
) -> Result<(), BodyMutationError> {
    work.lifecycle.steps_remaining = work
        .lifecycle
        .steps_remaining
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("recipe terminal steps"))?;
    work.lifecycle.bytes_remaining = work
        .lifecycle
        .bytes_remaining
        .checked_sub(u64::try_from(bytes).map_err(|_| invalid())?)
        .ok_or(ValueTreeError::ResourceLimited("recipe terminal bytes"))?;
    Ok(())
}
// Measure the actual paid DTO graph before cloning. Parser scratch and the
// caller's maximum decode capacities must not become a repeated copy fee.
pub(super) fn job_copy_layout(
    job: &RecipeJobState,
    common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
) -> Result<usize, BodyMutationError> {
    let mut n = std::mem::size_of::<RecipeJobState>();
    let mut add = |bytes: usize| -> Result<(), BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        n = n.checked_add(bytes).ok_or_else(invalid)?;
        Ok(())
    };
    for text in [&job.requester_agent_id, &job.factory_id, &job.recipe_id] {
        add(text.len())?;
    }
    for text in [
        &job.emergency_power_world_binding,
        &job.power_owner_agent_id,
    ]
    .into_iter()
    .flatten()
    {
        add(text.len())?;
    }
    if let Some(link) = &job.industrial_root_ref {
        for text in [
            &link.world_binding,
            &link.domain,
            &link.schema,
            &link.id,
            &link.digest,
        ] {
            add(text.len())?;
        }
    }
    if let Some(origin) = &job.committed_recipe_origin {
        let submission = &origin.submission;
        for text in [
            &origin.consensus_submitter_player_id,
            &origin.action_payload_hash,
            &origin.action_root,
            &submission.verified_player_id,
            &submission.public_key,
            &submission.requester_agent_id,
            &submission.factory_id,
            &submission.recipe_id,
        ] {
            add(text.len())?;
        }
        if let Some(text) = &submission.hosted_registration_nonce {
            add(text.len())?;
        }
    }
    for ledger in [&job.consume_ledger, &job.output_ledger] {
        if let crate::MaterialLedgerId::Agent(s)
        | crate::MaterialLedgerId::Site(s)
        | crate::MaterialLedgerId::Factory(s) = ledger
        {
            add(s.len())?;
        }
    }
    for list in [&job.consume, &job.produce, &job.byproducts] {
        add(list
            .len()
            .checked_mul(std::mem::size_of::<crate::MaterialStack>())
            .ok_or_else(invalid)?)?;
        for stack in list {
            add(stack.kind.len())?;
        }
    }
    for list in [
        &job.bottleneck_tags,
        &job.logistics_route_ids,
        &job.logistics_path_ids,
    ] {
        add(list
            .len()
            .checked_mul(std::mem::size_of::<String>())
            .ok_or_else(invalid)?)?;
        for text in list {
            add(text.len())?;
        }
    }
    Ok(n)
}

pub(super) fn factory_copy_layout(
    factory: &FactoryState,
    work: &mut FactoryScheduleWorkV1,
) -> Result<usize, BodyMutationError> {
    use oasis7_world_state::{
        incremental_factory_records::{
            FactoryCellV1, FactoryCollectionV1, visit_factory_record_v1,
        },
        incremental_field_member::MemberPayloadRefV1,
    };
    let mut n = std::mem::size_of::<FactoryState>();
    visit_factory_record_v1::<WorldFieldOverlayErrorV1>(
        FactoryRecordKeyV1::Factory(&factory.factory_id),
        FactoryRecordRefV1::Factory(factory),
        &mut |cell, payload| {
            work.lifecycle.steps_remaining = work.lifecycle.steps_remaining.checked_sub(1).ok_or(
                ValueTreeError::ResourceLimited("recipe factory clone measurement"),
            )?;
            let bytes = match (cell, payload) {
                (FactoryCellV1::Collection(kind), MemberPayloadRefV1::U64(count)) => {
                    let slot = match kind {
                        FactoryCollectionV1::SpecTags
                        | FactoryCollectionV1::SnapshotBottleneckTags
                        | FactoryCollectionV1::SnapshotLogisticsRouteIds
                        | FactoryCollectionV1::SnapshotLogisticsPathIds => {
                            std::mem::size_of::<String>()
                        }
                        FactoryCollectionV1::SpecBuildCost
                        | FactoryCollectionV1::SnapshotConsume
                        | FactoryCollectionV1::SnapshotProduce
                        | FactoryCollectionV1::SnapshotByproducts => {
                            std::mem::size_of::<crate::MaterialStack>()
                        }
                        FactoryCollectionV1::ObligationBalancesBefore
                        | FactoryCollectionV1::ObligationBalancesAfter => {
                            return Err(ValueTreeError::Invalid(
                                "factory clone unexpected obligation",
                            )
                            .into());
                        }
                    };
                    usize::try_from(count)
                        .ok()
                        .and_then(|n| n.checked_mul(slot))
                        .ok_or(ValueTreeError::ResourceLimited("factory clone slots"))?
                }
                (_, MemberPayloadRefV1::Text(text)) => text.len(),
                (
                    _,
                    MemberPayloadRefV1::Ledger(
                        crate::MaterialLedgerId::Agent(text)
                        | crate::MaterialLedgerId::Site(text)
                        | crate::MaterialLedgerId::Factory(text),
                    ),
                ) => text.len(),
                _ => 0,
            };
            n = n
                .checked_add(bytes)
                .ok_or(ValueTreeError::ResourceLimited("factory clone layout"))?;
            Ok(())
        },
    )
    .map_err(overlay)?;
    Ok(n)
}

/// Maximum new snapshot payload from the already paid job. The complete
/// selected factory's clone has its own observed allocation bound below.
fn snapshot_layout(job: &RecipeJobState) -> Result<usize, BodyMutationError> {
    let mut n = std::mem::size_of::<oasis7_world_state::FactoryProductionSnapshot>() + 4096;
    for text in [&job.factory_id, &job.recipe_id] {
        n = n.checked_add(text.len()).ok_or_else(invalid)?;
    }
    for list in [&job.consume, &job.produce, &job.byproducts] {
        n = n
            .checked_add(
                list.len()
                    .checked_mul(512 + 2 * std::mem::size_of::<crate::MaterialStack>())
                    .ok_or_else(invalid)?,
            )
            .ok_or_else(invalid)?;
        for stack in list {
            n = n
                .checked_add(stack.kind.len().checked_mul(2).ok_or_else(invalid)?)
                .ok_or_else(invalid)?;
        }
    }
    for list in [
        &job.bottleneck_tags,
        &job.logistics_route_ids,
        &job.logistics_path_ids,
    ] {
        n = n
            .checked_add(
                list.len()
                    .checked_mul(512 + 2 * std::mem::size_of::<String>())
                    .ok_or_else(invalid)?,
            )
            .ok_or_else(invalid)?;
        for text in list {
            n = n
                .checked_add(text.len().checked_mul(2).ok_or_else(invalid)?)
                .ok_or_else(invalid)?;
        }
    }
    for ledger in [&job.consume_ledger, &job.output_ledger] {
        if let crate::MaterialLedgerId::Agent(s)
        | crate::MaterialLedgerId::Site(s)
        | crate::MaterialLedgerId::Factory(s) = ledger
        {
            n = n.checked_add(s.len()).ok_or_else(invalid)?;
        }
    }
    // last_completed_recipe_id is a separate original clone.
    n.checked_add(job.recipe_id.len()).ok_or_else(invalid)
}

/// Apply the original completion projection to the actual selected factory.
/// A missing factory stays missing, as in the original replay; this function
/// cannot create a factory from a job or silently replace another record.
#[allow(clippy::too_many_arguments)]
pub(super) enum RecipeFactoryUpdateV1<'a> {
    Completed,
    Blocked { kind: &'a str, detail: &'a str },
}
pub(super) fn apply_factory_completion_v1<M, P, V, W, D, E, C, CW>(
    job: &RecipeJobState,
    update: RecipeFactoryUpdateV1<'_>,
    sources: &FactoryScheduleSourcesV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut FactoryScheduleWorkV1,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<FactoryState>>, BodyMutationError>
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
    spend(work, job.factory_id.len())?;
    let now = step.time();
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
    let before = oasis7_world_state::incremental_factory_schedule_factory::recover_current_schedule_complete_factory_v1(
        &job.factory_id, sources.lifecycle.factories, &mut ctx, &mut work.factory, &guard, live)
        .map_err(overlay)?;
    let Some(before) = before else {
        return Ok(None);
    };
    let layout = factory_copy_layout(&before, work)?
        .checked_add(match update {
            RecipeFactoryUpdateV1::Completed => snapshot_layout(job)?,
            RecipeFactoryUpdateV1::Blocked { kind, detail } => {
                kind.len().checked_add(detail.len()).ok_or_else(invalid)?
            }
        })
        .ok_or_else(invalid)?;
    let entries = job
        .consume
        .len()
        .checked_add(job.produce.len())
        .and_then(|n| n.checked_add(job.byproducts.len()))
        .and_then(|n| n.checked_add(job.bottleneck_tags.len()))
        .and_then(|n| n.checked_add(job.logistics_route_ids.len()))
        .and_then(|n| n.checked_add(job.logistics_path_ids.len()))
        .ok_or_else(invalid)?;
    // Blocking does not construct or normalize the completion snapshot.
    let entries = if matches!(update, RecipeFactoryUpdateV1::Blocked { .. }) {
        0
    } else {
        entries
    };
    let visits = entries
        .checked_add(1)
        .and_then(|n| n.checked_mul(n))
        .and_then(|n| n.checked_add(32))
        .ok_or_else(invalid)?;
    work.lifecycle.steps_remaining = work
        .lifecycle
        .steps_remaining
        .checked_sub(
            u64::try_from(visits.checked_add(layout).ok_or_else(invalid)?)
                .map_err(|_| invalid())?,
        )
        .ok_or(ValueTreeError::ResourceLimited(
            "recipe snapshot normalization visits",
        ))?;
    // Covers actual selected clone bytes and the original bounded BTree
    // normalization comparisons before entering that unchanged pure kernel.
    spend(
        work,
        layout
            .checked_mul(entries.checked_add(1).ok_or_else(invalid)?)
            .ok_or_else(invalid)?,
    )?;
    let fee = ctx
        .values
        .memory
        .reserve(layout, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    if (ctx.cancelled)() || (write.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let mut after = (*before).clone();
    match update {
        RecipeFactoryUpdateV1::Completed => {
            oasis7_world_state::state::industry_transition::complete_recipe_factory_production_v1(
                &mut after, job, now,
            )
        }
        RecipeFactoryUpdateV1::Blocked { kind, detail } => {
            oasis7_world_state::state::industry_transition::block_recipe_factory_production_v1(
                &mut after,
                Some(job),
                kind,
                detail,
                now,
            )
        }
    }
    let after = fee.retain(after);
    let guard = ctx.transaction.guard();
    replace_factory_record_overlay_v1(
        FactoryRecordChangeV1 {
            key: FactoryRecordKeyV1::Factory(&job.factory_id),
            expected: Some(FactoryRecordRefV1::Factory(&before)),
            after: Some(FactoryRecordRefV1::Factory(&after)),
            max_current_payload_bytes: work.lifecycle.max_scalar_bytes as u64,
        },
        &mut ctx,
        write,
        &mut work.lifecycle.factory,
        &guard,
        live,
    )
    .map_err(overlay)?;
    Ok(Some(after))
}

/// Read and compare an already persisted complete receipt before considering
/// settlement replay. This preserves the original conflict-before-idempotency
/// ordering and uses the original field visitor/resolver, not a second codec.
#[allow(clippy::too_many_arguments)]
fn check_receipt_v1<M, P, V, W, D, E, C>(
    receipt: &crate::RecipeCompletionReceiptV1,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    work: &mut FactoryScheduleWorkV1,
    live: &ExecutionFence,
) -> Result<(bool, bool), BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    use oasis7_world_state::{incremental_industry_jobs::*, incremental_world_id_sets::*};
    spend(work, 0)?;
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
    let key = IndustryRecordKeyV1 {
        kind: IndustryRecordKindV1::Completion,
        id: receipt.job_id,
    };
    let exists = read_industry_record_root_overlay_v1(key, &mut ctx)
        .map_err(overlay)?
        .is_some();
    if exists {
        let guard = ctx.transaction.guard();
        verify_industry_record_overlay_v1(
            key,
            IndustryRecordRefV1::Completion(receipt),
            &mut ctx,
            &mut work.lifecycle.industry,
            &guard,
            live,
        )
        .map_err(|error| match error {
            WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(
                "industry expected record",
            )) => state_error(
                ctx.values.memory,
                "recipe completion conflicts with persisted receipt: job_id=".len() + 20,
                format_args!(
                    "recipe completion conflicts with persisted receipt: job_id={}",
                    receipt.job_id
                ),
            ),
            other => overlay(other),
        })?;
    }
    let settled = world_id_set_contains_overlay_v1(
        WorldIdSetFieldV1::SettledRecipeJobs,
        WorldIdSetKeyRefV1::Action(receipt.job_id),
        &mut ctx,
    )
    .map_err(overlay)?;
    Ok((exists, settled))
}

/// Final commit of the original pending/settled/order/receipt rows. The caller
/// has completed output, production, progress and requester activity first.
#[allow(clippy::too_many_arguments)]
pub(super) fn settle_recipe_rows_v1<M, P, V, W, D, E, C, CW>(
    job: &RecipeJobState,
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
        incremental_industry_jobs::*, incremental_world_id_sets::*,
        incremental_world_number_maps::*, incremental_world_u64::*,
    };
    spend(work, 0)?;
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
    let pending_key = IndustryRecordKeyV1 {
        kind: IndustryRecordKindV1::Recipe,
        id: job.job_id,
    };
    let guard = ctx.transaction.guard();
    verify_industry_record_overlay_v1(
        pending_key,
        IndustryRecordRefV1::Recipe(job),
        &mut ctx,
        &mut work.lifecycle.industry,
        &guard,
        live,
    )
    .map_err(overlay)?;
    let expected_root =
        read_industry_record_root_overlay_v1(pending_key, &mut ctx).map_err(overlay)?;
    if expected_root.is_none() {
        return Err(invalid());
    }
    let guard = ctx.transaction.guard();
    delete_industry_record_overlay_v1(
        IndustryRecordDeleteV1 {
            key: pending_key,
            expected_root,
            qualification: sources.lifecycle.industry,
        },
        &mut ctx,
        write,
        &mut work.lifecycle.industry,
        &guard,
        live,
    )
    .map_err(overlay)?;
    let guard = ctx.transaction.guard();
    change_world_id_set_overlay_v1(
        WorldIdSetChangeV1 {
            field: WorldIdSetFieldV1::SettledRecipeJobs,
            key: WorldIdSetKeyRefV1::Action(job.job_id),
            expected: false,
            after: true,
        },
        &mut ctx,
        write,
        &guard,
        live,
    )
    .map_err(overlay)?;
    let order = read_world_number_map_overlay_v1(
        WorldNumberMapFieldV1::IndustrySettlementOrders,
        WorldNumberMapKeyRefV1::Action(job.job_id),
        &mut ctx,
    )
    .map_err(overlay)?;
    let next = read_world_u64_overlay_v1(WorldU64FieldV1::NextIndustrySettlementOrder, &mut ctx)
        .map_err(overlay)?;
    if order.is_none() {
        let guard = ctx.transaction.guard();
        change_world_number_map_overlay_v1(
            WorldNumberMapChangeV1 {
                field: WorldNumberMapFieldV1::IndustrySettlementOrders,
                key: WorldNumberMapKeyRefV1::Action(job.job_id),
                expected: None,
                after: Some(WorldNumberV1::Unsigned(next)),
            },
            &mut ctx,
            write,
            &guard,
            live,
        )
        .map_err(overlay)?;
        let guard = ctx.transaction.guard();
        change_world_u64_overlay_v1(
            WorldU64FieldV1::NextIndustrySettlementOrder,
            next,
            next.saturating_add(1),
            &mut ctx,
            write,
            &guard,
            live,
        )
        .map_err(overlay)?;
    }
    ctx.transaction
        .validate_guard(&ctx.transaction.guard(), live)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn commit_recipe_records_v1<M, P, V, W, D, E, C, CW>(
    job: &RecipeJobState,
    receipt: &crate::RecipeCompletionReceiptV1,
    receipt_exists: bool,
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
    use oasis7_world_state::incremental_industry_jobs::*;
    settle_recipe_rows_v1(job, sources, step, read, write, work, live)?;
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
    if !receipt_exists {
        let guard = ctx.transaction.guard();
        create_industry_record_overlay_v1(
            IndustryRecordCreateV1 {
                key: IndustryRecordKeyV1 {
                    kind: IndustryRecordKindV1::Completion,
                    id: job.job_id,
                },
                after: IndustryRecordRefV1::Completion(receipt),
                qualification: sources.lifecycle.industry,
            },
            &mut ctx,
            write,
            &mut work.lifecycle.industry,
            &guard,
            live,
        )
        .map_err(overlay)?;
    } else {
        let guard = ctx.transaction.guard();
        verify_industry_record_overlay_v1(
            IndustryRecordKeyV1 {
                kind: IndustryRecordKindV1::Completion,
                id: job.job_id,
            },
            IndustryRecordRefV1::Completion(receipt),
            &mut ctx,
            &mut work.lifecycle.industry,
            &guard,
            live,
        )
        .map_err(overlay)?;
    }
    ctx.transaction
        .validate_guard(&ctx.transaction.guard(), live)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) enum RecipeProgressUpdateV1 {
    Completed,
    Blocked,
}
#[allow(clippy::too_many_arguments)]
pub(super) fn apply_recipe_progress_v1<M, P, V, W, D, E, C, CW>(
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
    spend(work, job.factory_id.len())?;
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
        &job.factory_id,
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
    match update {
        RecipeProgressUpdateV1::Completed => {
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
        RecipeProgressUpdateV1::Blocked => {
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

/// Consume the genuine terminal producer, apply every RecipeCompleted state
/// effect, and only then create the source accepted by the counted publisher.
#[allow(clippy::too_many_arguments)]
pub(super) fn complete_current_due_recipe_v1<M, P, V, W, D, E, C, PE, PC, CW, ME>(
    selected: ValidatedDueRecipeTerminalV1,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<
        '_,
        M,
        ME,
    >,
    sources: &FactoryScheduleSourcesV1<'_>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut FactoryScheduleWorkV1,
    common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PreparedNormalRecipeCompletedV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
{
    let now = step.time();
    let identity = common.work().identity();
    selected.validate(step.transaction, common.memory(), now, identity, live)?;
    let job_bound = job_copy_layout(&selected.pending, common)?;
    let event_bound = job_bound
        .checked_add(std::mem::size_of::<crate::WorldEvent>())
        .ok_or_else(invalid)?;
    let receipt_bound = job_bound
        .checked_add(std::mem::size_of::<crate::RecipeCompletionReceiptV1>())
        .ok_or_else(invalid)?;
    let outer_bound = std::mem::size_of::<PreparedNormalRecipeCompletedV1>();
    let phase = event_bound
        .checked_add(receipt_bound)
        .and_then(|n| n.checked_add(outer_bound))
        .ok_or_else(invalid)?;
    common
        .charge(
            u64::try_from(phase).map_err(|_| invalid())?,
            u64::try_from(phase).map_err(|_| invalid())?,
        )
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let event_fee = common
        .memory()
        .reserve(event_bound, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let receipt_fee = common
        .memory()
        .reserve(receipt_bound, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let outer_fee = common
        .memory()
        .reserve(outer_bound, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let job = &*selected.pending;
    let body = crate::World::recipe_completion_body(job.clone());
    let crate::WorldEventBody::Domain(domain) = &body else {
        return Err(invalid());
    };
    let receipt = receipt_fee.retain(
        oasis7_world_state::state::industry_transition::recipe_completion_receipt_from_event_v1(
            domain,
        )
        .ok_or_else(invalid)?,
    );
    let (event, guard) = crate::world::incremental_step::factory_schedule_write_io::paid(
        step,
        read,
        parent,
        write,
        materials,
        work,
        common,
        live,
        |step, read, parent, write, materials, work| {
            let (receipt_exists, settled) = check_receipt_v1(&receipt, step, read, work, live)?;
            if !settled {
                if now < job.ready_at {
                    return Err(state_error(
                        step.mutation.memory,
                        "recipe completion is early: job_id= ready_at= now=".len() + 3 * 20,
                        format_args!(
                            "recipe completion is early: job_id={} ready_at={} now={}",
                            job.job_id, job.ready_at, now
                        ),
                    ));
                }
                // The original output preflight considers produce and byproducts
                // together, then applies their aggregate to the real ledger.
                let count = job
                    .produce
                    .len()
                    .checked_add(job.byproducts.len())
                    .ok_or_else(invalid)?;
                let bytes = job_bound
                    .checked_add(
                        count
                            .checked_mul(std::mem::size_of::<crate::MaterialStack>())
                            .ok_or_else(invalid)?,
                    )
                    .ok_or_else(invalid)?;
                spend(work, bytes)?;
                let output_fee = step
                    .mutation
                    .memory
                    .reserve(bytes, MemoryClass::Ordinary)
                    .map_err(ValueTreeError::Memory)?;
                let mut output = Vec::with_capacity(count);
                if output.capacity() != count {
                    return Err(invalid());
                }
                output.extend_from_slice(&job.produce);
                output.extend_from_slice(&job.byproducts);
                let output = output_fee.retain(output);
                let plan = crate::world::incremental_action_factory_lifecycle::material_plan::prepare_with_prefixes_v1(
                    step, materials, &job.output_ledger, &output, false, read, &mut work.lifecycle, live,
                    "recipe output preflight failed: ", "recipe produce failed: ", "recipe produce failed: ")?;
                crate::world::incremental_action_factory_lifecycle::material_plan::apply(
                    &plan,
                    &job.output_ledger,
                    step,
                    materials,
                    read,
                    &mut work.lifecycle,
                    live,
                )?;
                let factory = apply_factory_completion_v1(
                    job,
                    RecipeFactoryUpdateV1::Completed,
                    sources,
                    step,
                    read,
                    write,
                    work,
                    live,
                )?;
                apply_recipe_progress_v1(
                    job,
                    job_bound,
                    RecipeProgressUpdateV1::Completed,
                    factory.as_deref(),
                    sources,
                    step,
                    read,
                    write,
                    work,
                    live,
                )?;
                update_requester_v1(
                    job,
                    factory
                        .as_deref()
                        .is_some_and(|f| f.production.active_jobs == 0),
                    sources,
                    step,
                    read,
                    parent,
                    work,
                    live,
                )?;
                commit_recipe_records_v1(
                    job,
                    &receipt,
                    receipt_exists,
                    sources,
                    step,
                    read,
                    write,
                    work,
                    live,
                )?;
            } else {
                // A settled retry still performs original World/legacy
                // normalization, without repeating output credits.
                let guard = step.transaction.guard();
                materials.contribute(step.transaction, step.values, step.mutation, &guard, live)?;
            }
            let sequence = step
                .read_allocator(crate::world::incremental_allocators::RuntimeAllocatorV1::Event)?;
            let (id, _, _) = crate::World::preview_next_event_id(sequence.next, sequence.era);
            let event = event_fee.retain(crate::WorldEvent {
                id,
                time: now,
                caused_by: None,
                body,
            });
            let guard = step.transaction.guard();
            step.transaction.validate_guard(&guard, live)?;
            Ok((event, guard))
        },
    )?;
    Ok(PreparedNormalRecipeCompletedV1 {
        event,
        guard,
        identity,
        now,
        memory: common.memory().clone(),
        _event_phase: outer_fee,
        _pending: selected.pending,
        _receipt: receipt,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn update_requester_v1<M, P, V, W, D, E, C, PE, PC>(
    job: &RecipeJobState,
    no_active: bool,
    sources: &FactoryScheduleSourcesV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut FactoryScheduleWorkV1,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    use oasis7_world_state::{
        incremental_agent_body::{AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1},
        incremental_agent_subtree::{CurrentAgentPresenceV1, read_current_agent_presence_v1},
    };
    let exists = {
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
        read_current_agent_presence_v1(
            &job.requester_agent_id,
            sources.lifecycle.agents,
            sources.roots,
            &mut ctx,
            parent,
            &mut work.lifecycle.pending_entries_remaining,
            &guard,
            live,
        )
        .map_err(overlay)?
            == CurrentAgentPresenceV1::Existing
    };
    if !exists {
        return Ok(());
    }
    if no_active {
        crate::world::incremental_normal_action_factory_schedule::set_completed_recipe_idle_v1(
            &job.requester_agent_id,
            sources.lifecycle.agents,
            step,
            read,
            work,
            live,
        )?;
    }
    spend(
        work,
        job.requester_agent_id
            .len()
            .checked_add(2 * AGENT_BODY_VALUE_BYTES)
            .ok_or_else(invalid)?,
    )?;
    let key = AgentBodyKeyV1::new(&job.requester_agent_id).map_err(BodyMutationError::Invalid)?;
    let body = AgentBodyValueV1::from_canonical_bytes(
        &crate::world::incremental_body_event::read_fixed::<AGENT_BODY_VALUE_BYTES, _, _, _, _>(
            step.transaction,
            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
            key.as_bytes(),
            step.values,
            step.mutation,
        )?,
    )
    .map_err(BodyMutationError::Invalid)?;
    let after = crate::world::incremental_body::write_body_value(
        &AgentBodyValueV1 {
            body_view: body.body_view,
            last_active: step.time(),
        },
        step.mutation,
    )?;
    step.transaction.write(
        oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
        key.as_bytes(),
        Some(after),
    )?;
    Ok(())
}
