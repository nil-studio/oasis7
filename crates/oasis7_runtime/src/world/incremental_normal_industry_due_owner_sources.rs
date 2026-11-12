//! Refresh the actual Current order source before each terminal owner check.
//! The callback receives only borrowed data after both child I/O phases finish.
use super::*;
use crate::world::{
    incremental_factory_directory_orders::{
        AcceptedFactoryOrderDirectoryV1, FactoryOrderDirectoryErrorV1, FactoryOrderDirectoryWorkV1,
        open_current_factory_order_directory_v1,
    },
    incremental_industrial_eligibility::{CurrentStateOwnerSourcesV1, IndustrialEligibilityWorkV1},
    incremental_normal_action_factory_schedule::FactoryScheduleSourcesV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_value::{ValueNodeReader, ValueReadContext, ValueTreeError},
};
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_rule_composite_family::bind_current_agent_orders_v1,
    incremental_world_rules_family::WorldRulesQualificationV1,
};
fn limited() -> BodyMutationError {
    ValueTreeError::ResourceLimited("due owner directory Common work").into()
}
fn error(e: FactoryOrderDirectoryErrorV1) -> BodyMutationError {
    match e {
        FactoryOrderDirectoryErrorV1::Execution(e) => e.into(),
        FactoryOrderDirectoryErrorV1::Value(e) => e.into(),
        FactoryOrderDirectoryErrorV1::Overlay(e) => super::recipe_commit::overlay(e),
        FactoryOrderDirectoryErrorV1::Body(e) => e,
    }
}
fn lanes<M, R, E, C>(
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut IndustrialEligibilityWorkV1,
    orders: &mut FactoryOrderDirectoryWorkV1,
    cap: Option<(u64, u64)>,
) -> Result<([u64; 5], [usize; 2], [u64; 6]), BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    if let Some((steps, bytes)) = cap {
        for lane in [
            &mut ctx.values.io.nodes_remaining,
            &mut work.scan.nodes_remaining,
            &mut work.scan.leaves_remaining,
            &mut work.typed.json.cells_remaining,
            &mut work.typed.json.json_nodes_remaining,
        ] {
            *lane = (*lane).min(steps);
        }
        let count = usize::try_from(steps).unwrap_or(usize::MAX);
        work.pending_entries_remaining = work.pending_entries_remaining.min(count);
        orders.pending_entries_remaining = orders.pending_entries_remaining.min(count);
        for lane in [
            &mut ctx.values.io.read_bytes_remaining,
            &mut ctx.values.io.write_bytes_remaining,
            &mut ctx.key_work.bytes_remaining,
            &mut work.scan.read_bytes_remaining,
            &mut orders.ids.comparison_bytes_remaining,
            &mut orders.merge_comparison_bytes_remaining,
        ] {
            *lane = (*lane).min(bytes);
        }
    }
    Ok((
        [
            ctx.values.io.nodes_remaining,
            work.scan.nodes_remaining,
            work.scan.leaves_remaining,
            work.typed.json.cells_remaining,
            work.typed.json.json_nodes_remaining,
        ],
        [
            work.pending_entries_remaining,
            orders.pending_entries_remaining,
        ],
        [
            ctx.values.io.read_bytes_remaining,
            ctx.values.io.write_bytes_remaining,
            ctx.key_work.bytes_remaining,
            work.scan.read_bytes_remaining,
            orders.ids.comparison_bytes_remaining,
            orders.merge_comparison_bytes_remaining,
        ],
    ))
}
fn paid<M, R, E, C, T>(
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut IndustrialEligibilityWorkV1,
    orders: &mut FactoryOrderDirectoryWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    run: impl FnOnce(
        &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, &mut E, &mut dyn FnMut() -> bool>,
        &mut IndustrialEligibilityWorkV1,
        &mut FactoryOrderDirectoryWorkV1,
    ) -> Result<T, FactoryOrderDirectoryErrorV1>,
) -> Result<T, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    if !ctx.values.memory.same_pool(common.memory())
        || !ctx.transaction.uses_budget(common.memory())
        || ctx.values.class != oasis7_proto::memory_budget::MemoryClass::Ordinary
    {
        return Err(BodyMutationError::Invalid("due owner directory pool"));
    }
    common
        .consume_steps(1)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let (_, meter, stop) = common.parts();
    let child = meter
        .begin_child()
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let before = lanes(
        ctx,
        work,
        orders,
        Some((
            child.remaining_steps() / 7,
            child.remaining_read_bytes() / 6,
        )),
    )?;
    let result = {
        let mut cancelled = || stop() || (ctx.cancelled)();
        let mut local = WorldFieldOverlayContextV1 {
            transaction: &mut *ctx.transaction,
            values: ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut cancelled as &mut dyn FnMut() -> bool,
        };
        run(&mut local, work, orders)
    };
    let after = lanes(ctx, work, orders, None)?;
    let accounted = (|| {
        let mut steps = 0u64;
        let mut bytes = 0u64;
        for (a, b) in before.0.into_iter().zip(after.0) {
            steps = steps
                .checked_add(a.checked_sub(b).ok_or_else(limited)?)
                .ok_or_else(limited)?;
        }
        for (a, b) in before.1.into_iter().zip(after.1) {
            steps = steps
                .checked_add(
                    u64::try_from(a.checked_sub(b).ok_or_else(limited)?).map_err(|_| limited())?,
                )
                .ok_or_else(limited)?;
        }
        for (a, b) in before.2.into_iter().zip(after.2) {
            bytes = bytes
                .checked_add(a.checked_sub(b).ok_or_else(limited)?)
                .ok_or_else(limited)?;
        }
        child
            .consume(steps, bytes)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))
    })();
    if let Err(accounting) = accounted {
        return match result {
            Err(source) => Err(error(source)),
            Ok(_) => Err(accounting),
        };
    }
    child.finish();
    let value = result.map_err(error)?;
    common
        .consume_steps(0)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    Ok(value)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn with_current_due_owner_sources_v1<M, P, R, V, E, C, PE, PC, T>(
    accepted: &ReservedValue<AcceptedFactoryOrderDirectoryV1>,
    rules: &ReservedValue<WorldRulesQualificationV1>,
    sources: &FactoryScheduleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut IndustrialEligibilityWorkV1,
    orders: &mut FactoryOrderDirectoryWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
    consume: impl FnOnce(
        &CurrentStateOwnerSourcesV1<'_, '_>,
        &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
        &mut IndustrialEligibilityWorkV1,
        &mut FormalBoundedContextV1<'_>,
    ) -> Result<T, BodyMutationError>,
) -> Result<T, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    V: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
{
    let current = paid(ctx, work, orders, common, |ctx, work, _| {
        let guard = ctx.transaction.guard();
        Ok(bind_current_agent_orders_v1(
            rules.orders(),
            sources.roots,
            parent.state,
            ctx,
            &mut work.scan,
            &mut work.typed,
            &guard,
            live,
        )?)
    })?;
    let directory = paid(ctx, work, orders, common, |ctx, work, orders| {
        let guard = ctx.transaction.guard();
        open_current_factory_order_directory_v1(
            accepted,
            &current,
            parent.state,
            ctx,
            &mut work.scan,
            &mut work.typed,
            orders,
            &guard,
            live,
        )
    })?;
    let current_sources = CurrentStateOwnerSourcesV1 {
        roots: sources.roots,
        orders: &directory,
        agents: sources.lifecycle.agents,
        native: sources.lifecycle.native,
    };
    consume(&current_sources, ctx, parent, work, common)
}
