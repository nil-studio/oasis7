//! Actual selected Current pending jobs. This is an internal data producer;
//! receipt publication remains gated by the post-write completion owner.
use super::*;
use crate::world::incremental_normal_action_factory_schedule::{
    FactoryScheduleReadErrorV1, FactoryScheduleSourcesV1, FactoryScheduleWorkV1,
    with_factory_schedule_read_io_v1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_world_state::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_industry_jobs::{
        IndustryRecordKeyV1, IndustryRecordKindV1, read_industry_cell_overlay_v1,
    },
};

#[allow(clippy::too_many_arguments)]
pub(super) fn recover_current_pending_recipe_v1<M, P, R, V, E, C, PE, PC>(
    outer: u64,
    sources: &FactoryScheduleSourcesV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<ReservedValue<crate::RecipeJobState>, FactoryScheduleReadErrorV1>
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
    let guard = context.transaction.guard();
    context
        .transaction
        .validate_guard(&guard, live)
        .map_err(BodyMutationError::from)?;
    let max_text = work.lifecycle.max_scalar_bytes;
    let max_entries = work.lifecycle.max_material_entries;
    let layout = super::recipe_read::recipe_decode_layout_v1(max_text, max_entries)?;
    common
        .charge(
            1,
            u64::try_from(layout).map_err(|_| BodyMutationError::Invalid("due recipe layout"))?,
        )
        .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
    with_factory_schedule_read_io_v1(
        sources.roots,
        context,
        parent,
        work,
        common,
        |ctx, _parent, work| {
            if !sources.lifecycle.industry.uses_budget(ctx.values.memory)
                || sources.lifecycle.industry.allocation_class()
                    != oasis7_proto::memory_budget::MemoryClass::Ordinary
            {
                return Err(BodyMutationError::Invalid("due recipe source pool").into());
            }
            sources
                .lifecycle
                .industry
                .validate_binding(&sources.roots.digest().map_err(BodyMutationError::from)?)?;
            let memory = ctx.values.memory.clone();
            let result = super::recipe_read::decode_pending_recipe_v1(
                outer,
                &memory,
                max_text,
                max_entries,
                |cell, output| {
                    work.lifecycle.industry.cells_remaining = work
                        .lifecycle
                        .industry
                        .cells_remaining
                        .checked_sub(1)
                        .ok_or(ValueTreeError::ResourceLimited("due recipe Current cells"))?;
                    read_industry_cell_overlay_v1(
                        IndustryRecordKeyV1 {
                            kind: IndustryRecordKindV1::Recipe,
                            id: outer,
                        },
                        cell,
                        ctx,
                        max_text as u64,
                        &mut { output },
                    )
                    .map_err(|error| match error {
                        WorldFieldOverlayErrorV1::Execution(error) => error.into(),
                        WorldFieldOverlayErrorV1::Value(error) => error.into(),
                        WorldFieldOverlayErrorV1::Member(error) => {
                            BodyMutationError::Original(Box::new(error))
                        }
                    })
                },
            )?;
            ctx.transaction
                .validate_guard(&ctx.transaction.guard(), live)
                .map_err(BodyMutationError::from)?;
            Ok(result)
        },
    )
}
