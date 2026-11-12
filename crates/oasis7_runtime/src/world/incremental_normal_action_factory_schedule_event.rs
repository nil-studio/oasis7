//! Paid copies of the original industrial pending records. Private data only;
//! Selected/Accepted identity and Current effects are checked by the committer.
use crate::world::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{FactoryBuildJobState, MaterialLedgerId, MaterialStack, RecipeJobState};
use std::mem::{align_of, size_of};
#[derive(Debug)]
struct RecipeOriginErrorV1 {
    cause: crate::WorldError,
    _fee: oasis7_proto::memory_budget::MemoryReservation,
}
impl std::fmt::Display for RecipeOriginErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.cause)
    }
}
impl std::error::Error for RecipeOriginErrorV1 {}
fn limited() -> BodyMutationError {
    ValueTreeError::ResourceLimited("factory schedule record layout").into()
}
fn add(total: &mut usize, n: usize) -> Result<(), BodyMutationError> {
    *total = total.checked_add(n).ok_or_else(limited)?;
    Ok(())
}
fn charge(ctx: &mut FormalBoundedContextV1<'_>, n: usize) -> Result<(), BodyMutationError> {
    ctx.consume_steps(1)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    ctx.consume_read_bytes(u64::try_from(n).map_err(|_| limited())?)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    Ok(())
}
fn text(
    total: &mut usize,
    value: &str,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    charge(ctx, value.len())?;
    add(total, value.len())
}
fn texts(
    total: &mut usize,
    values: &[String],
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    add(
        total,
        values
            .len()
            .checked_mul(size_of::<String>())
            .ok_or_else(limited)?,
    )?;
    for value in values {
        text(total, value, ctx)?;
    }
    Ok(())
}
fn stacks(
    total: &mut usize,
    values: &[MaterialStack],
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    add(
        total,
        values
            .len()
            .checked_mul(size_of::<MaterialStack>())
            .ok_or_else(limited)?,
    )?;
    for value in values {
        text(total, &value.kind, ctx)?;
    }
    Ok(())
}
fn ledger(
    total: &mut usize,
    value: &MaterialLedgerId,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    match value {
        MaterialLedgerId::World => Ok(()),
        MaterialLedgerId::Agent(v) | MaterialLedgerId::Site(v) | MaterialLedgerId::Factory(v) => {
            text(total, v, ctx)
        }
    }
}
// Locked Rust B=6/CAP=11: full internal-node upper bound with all-field
// alignment padding, without assuming LeafNode repr(Rust) field placement.
// One node per input plus one covers empty-but-allocated source roots too.
fn map_layout(entries: usize) -> Result<usize, BodyMutationError> {
    let alignment = align_of::<String>()
        .max(align_of::<i64>())
        .max(align_of::<usize>());
    let node = 11usize
        .checked_mul(size_of::<String>() + size_of::<i64>())
        .and_then(|n| n.checked_add(13 * size_of::<usize>() + 2 * size_of::<u16>()))
        .and_then(|n| n.checked_add(7 * (alignment - 1)))
        .ok_or_else(limited)?;
    entries
        .checked_add(1)
        .and_then(|n| n.checked_mul(node))
        .ok_or_else(limited)
}
pub(super) fn build_job_from_event_v1(
    body: &crate::WorldEventBody,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<Option<ReservedValue<FactoryBuildJobState>>, BodyMutationError> {
    let crate::WorldEventBody::Domain(crate::DomainEvent::FactoryBuildStarted {
        job_id,
        builder_agent_id,
        site_id,
        spec,
        consume_ledger,
        ready_at,
        contract_version,
        site_authority_revision,
        site_location_id,
        location_anchor_revision,
        construction_power_obligation,
    }) = body
    else {
        return Ok(None);
    };
    let mut n = size_of::<FactoryBuildJobState>();
    for value in [
        builder_agent_id,
        site_id,
        &spec.factory_id,
        &spec.display_name,
    ] {
        text(&mut n, value, ctx)?;
    }
    texts(&mut n, &spec.tags, ctx)?;
    stacks(&mut n, &spec.build_cost, ctx)?;
    ledger(&mut n, consume_ledger, ctx)?;
    if let Some(value) = site_location_id {
        text(&mut n, value, ctx)?;
    }
    if let Some(value) = construction_power_obligation {
        text(&mut n, &value.payer_agent_id, ctx)?;
        text(&mut n, &value.profile_key, ctx)?;
        if let Some(value) = &value.material_ledger {
            ledger(&mut n, value, ctx)?;
        }
        for map in [
            &value.material_balances_before,
            &value.material_balances_after,
        ]
        .into_iter()
        .flatten()
        {
            add(&mut n, map_layout(map.len())?)?;
            for key in map.keys() {
                text(&mut n, key, ctx)?;
                charge(ctx, key.len().checked_mul(map.len()).ok_or_else(limited)?)?;
            }
        }
    }
    let fee = ctx
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    charge(ctx, n)?;
    Ok(Some(fee.retain(FactoryBuildJobState {
        job_id: *job_id,
        builder_agent_id: builder_agent_id.clone(),
        site_id: site_id.clone(),
        spec: spec.clone(),
        consume_ledger: consume_ledger.clone(),
        ready_at: *ready_at,
        contract_version: contract_version.unwrap_or_default(),
        site_authority_revision: *site_authority_revision,
        site_location_id: site_location_id.clone(),
        location_anchor_revision: *location_anchor_revision,
        construction_power_obligation: construction_power_obligation.clone(),
    })))
}
pub(super) fn recipe_job_from_event_v1(
    body: &crate::WorldEventBody,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<Option<ReservedValue<RecipeJobState>>, BodyMutationError> {
    let crate::WorldEventBody::Domain(crate::DomainEvent::RecipeStarted {
        job_id,
        requester_agent_id,
        factory_id,
        recipe_id,
        accepted_batches,
        consume,
        produce,
        byproducts,
        power_required,
        power_owner_agent_id,
        duration_ticks,
        consume_ledger,
        output_ledger,
        bottleneck_tags,
        logistics_route_ids,
        logistics_path_ids,
        ready_at,
        emergency_power_obligation,
        committed_recipe_origin,
        ..
    }) = body
    else {
        return Ok(None);
    };
    // This is the first original replay predicate, before pending/settled,
    // slot, timing, material or power effects. Do not turn it into Rule denial.
    if let Some(origin) = committed_recipe_origin {
        for value in super::origin::origin_texts_v1(origin) {
            charge(ctx, value.len())?;
        }
    }
    for value in [requester_agent_id, factory_id, recipe_id] {
        charge(ctx, value.len())?;
    }
    let error_fee = ctx
        .memory()
        .reserve(
            size_of::<RecipeOriginErrorV1>() + "recipe start committed origin mismatch".len(),
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    oasis7_world_state::industry_transition::validate_recipe_started_origin_v1(
        committed_recipe_origin.as_ref(),
        requester_agent_id,
        factory_id,
        recipe_id,
    )
    .map_err(|cause| {
        BodyMutationError::Original(Box::new(RecipeOriginErrorV1 {
            cause,
            _fee: error_fee,
        }))
    })?;
    // This owner is reached only by direct ScheduleRecipe. The
    // emergency variants have their own real source/escrow committers.
    // Committed recipe correlation is ordinary metadata and remains intact.
    if emergency_power_obligation.is_some() {
        return Err(BodyMutationError::Invalid("direct ScheduleRecipe origin"));
    }
    let mut n = size_of::<RecipeJobState>();
    if let Some(origin) = committed_recipe_origin {
        for value in super::origin::origin_texts_v1(origin) {
            text(&mut n, value, ctx)?;
        }
    }
    for value in [requester_agent_id, factory_id, recipe_id] {
        text(&mut n, value, ctx)?;
    }
    for values in [consume, produce, byproducts] {
        stacks(&mut n, values, ctx)?;
    }
    if let Some(value) = power_owner_agent_id {
        text(&mut n, value, ctx)?;
    }
    ledger(&mut n, consume_ledger, ctx)?;
    ledger(&mut n, output_ledger, ctx)?;
    for values in [bottleneck_tags, logistics_route_ids, logistics_path_ids] {
        texts(&mut n, values, ctx)?;
    }
    let fee = ctx
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    charge(ctx, n)?;
    Ok(Some(fee.retain(RecipeJobState {
        industrial_root_ref: None,
        emergency_power_world_binding: None,
        committed_recipe_origin: committed_recipe_origin.clone(),
        job_id: *job_id,
        requester_agent_id: requester_agent_id.clone(),
        factory_id: factory_id.clone(),
        recipe_id: recipe_id.clone(),
        accepted_batches: *accepted_batches,
        consume: consume.clone(),
        produce: produce.clone(),
        byproducts: byproducts.clone(),
        power_required: *power_required,
        power_owner_agent_id: power_owner_agent_id.clone(),
        duration_ticks: *duration_ticks,
        consume_ledger: consume_ledger.clone(),
        output_ledger: output_ledger.clone(),
        bottleneck_tags: bottleneck_tags.clone(),
        logistics_route_ids: logistics_route_ids.clone(),
        logistics_path_ids: logistics_path_ids.clone(),
        ready_at: *ready_at,
    })))
}
