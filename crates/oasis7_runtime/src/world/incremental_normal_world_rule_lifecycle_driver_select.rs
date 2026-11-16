//! One complete initial Current snapshot selection, in the original five
//! lane order. Only after this function finishes may any domain writer run.
use super::delivery::{
    self, IndustrialDeliveryErrorV1, IndustrialDeliverySourcesV1, IndustrialDeliveryWorkV1,
};
use super::driver_sources::NormalWorldRuleLifecycleSourcesV1;
use super::selection::LifecycleSeedListV1;
use super::work::{LifecycleRequestV1, OrderLifecycleChoiceV1};
use super::*;
use crate::world::{
    incremental_body_event::materials::CorePolicyMaterialContextV1,
    incremental_normal_tail::NormalTailWorkV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_industry_jobs::{
        IndustryRecordKeyV1, IndustryRecordKindV1, read_industry_record_root_overlay_v1,
    },
    incremental_world_rule_requests_current::read_current_observation_registry_v2,
    incremental_world_rules_family::{
        WorldRulesFamilyWorkV1, open_current_world_rule_lifecycle_directory_v1,
    },
    world_rules::AgentIndustrialPhaseV1,
};
use std::{collections::BTreeSet, mem::size_of};
#[allow(clippy::too_many_arguments)]
pub(super) fn capture_initial_lifecycle_seeds_v1<M, P, V, R, W, E, C, PE, PC, CW, ME>(
    sources: &NormalWorldRuleLifecycleSourcesV1<'_>,
    current: &IndustrialDeliverySourcesV1<'_, '_>,
    state: &M,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    work: &mut IndustrialDeliveryWorkV1,
    world: &mut WorldRulesFamilyWorkV1,
    extra: &mut emergency_close::sources::EmergencyServiceSourceWorkV1<'_>,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<LifecycleSeedListV1, IndustrialDeliveryErrorV1>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
{
    delivery::paid_io::run(
        ctx,
        parent,
        write,
        materials,
        work,
        common,
        |ctx, parent, _write, _materials, work, tail| {
            let cap = tail.remaining_steps() / 4;
            world.outer_records_remaining = world.outer_records_remaining.min(cap);
            world.remainder.cells_remaining = world.remainder.cells_remaining.min(cap);
            let before = (
                world.outer_records_remaining,
                world.remainder.cells_remaining,
            );
            let result: Result<LifecycleSeedListV1, IndustrialDeliveryErrorV1> = (|| {
                tail.charge(8192)?;
                let guard = ctx.transaction.guard();
                let directory = open_current_world_rule_lifecycle_directory_v1(
                    sources.rules,
                    sources.factory.roots,
                    state,
                    ctx,
                    &mut work.fixed_scan,
                    world,
                    &mut work.composite_read,
                    &mut work.eligibility.pending_entries_remaining,
                    &guard,
                    live,
                )?;
                directory.validate(ctx.transaction, sources.factory.roots, tail.memory(), live)?;
                let mut seeds = LifecycleSeedListV1::new(tail)?;
                // The original draft Material authorization validity is always false.
                // Actual Current metadata selects active records; the fixed Close producer
                // still reopens/revalidates every selected authorization and allocation.
                for row in directory.supply().authorizations() {
                    tail.charge_record()?;
                    if row.active {
                        seeds.push(
                            &row.provider_id,
                            LifecycleRequestV1::SupplyClose(&row.id),
                            tail,
                        )?;
                    }
                }
                // Settlement presence is keyed by the original outer shipment ID.
                for row in directory.supply_shipments() {
                    tail.charge_record()?;
                    if row.value.settled {
                        continue;
                    }
                    if read_industry_record_root_overlay_v1(
                        IndustryRecordKeyV1 {
                            kind: IndustryRecordKindV1::Settlement,
                            id: row.outer_id,
                        },
                        ctx,
                    )?
                    .is_none()
                    {
                        continue;
                    }
                    let count = directory.supply().authorizations().len();
                    let factor = (usize::BITS - count.max(1).leading_zeros()) as usize + 1;
                    tail.charge(row.value.authorization_id.len().checked_mul(factor).ok_or(
                        BodyMutationError::Invalid("lifecycle shipment provider search"),
                    )?)?;
                    let pos = directory
                        .supply()
                        .authorizations()
                        .binary_search_by(|auth| {
                            auth.outer_id.as_str().cmp(&row.value.authorization_id)
                        })
                        .map_err(|_| {
                            BodyMutationError::Invalid("shipment authorization missing")
                        })?;
                    let auth = &directory.supply().authorizations()[pos];
                    seeds.push(
                        &auth.provider_id,
                        LifecycleRequestV1::SupplySettle(row.outer_id),
                        tail,
                    )?;
                }
                if let Some(window) = now.checked_sub(60).filter(|_| now.is_multiple_of(60)) {
                    let eligible = directory
                        .emergency()
                        .minimum_services()
                        .filter(|row| {
                            row.registered_at
                                .checked_add(600)
                                .is_some_and(|time| time <= window)
                        })
                        .count();
                    let heap=oasis7_world_state::incremental_collection_layout::string_set_output_heap_v1(eligible)?.checked_add(eligible.checked_mul(size_of::<&str>()).ok_or(BodyMutationError::Invalid("lifecycle region slots"))?).ok_or(BodyMutationError::Invalid("lifecycle region heap"))?;
                    tail.charge(heap)?;
                    let _region_fee = tail
                        .memory()
                        .reserve(heap, oasis7_proto::memory_budget::MemoryClass::Ordinary)
                        .map_err(ValueTreeError::Memory)?;
                    let mut regions = BTreeSet::new();
                    let factor = (usize::BITS - eligible.max(1).leading_zeros()) as usize + 1;
                    for row in directory.emergency().minimum_services() {
                        tail.charge_record()?;
                        if row
                            .registered_at
                            .checked_add(600)
                            .is_some_and(|time| time <= window)
                        {
                            tail.charge(
                                row.region_id
                                    .len()
                                    .checked_mul(factor.checked_mul(12).ok_or(
                                        BodyMutationError::Invalid("lifecycle region comparisons"),
                                    )?)
                                    .ok_or(BodyMutationError::Invalid("lifecycle region bytes"))?,
                            )?;
                            regions.insert(row.region_id.as_str());
                        }
                    }
                    for region in regions {
                        let bytes = region
                            .len()
                            .checked_mul(2)
                            .and_then(|n| n.checked_add(256))
                            .ok_or(BodyMutationError::Invalid("lifecycle observation key"))?;
                        tail.charge(bytes)?;
                        let _key_fee = tail
                            .memory()
                            .reserve(bytes, oasis7_proto::memory_budget::MemoryClass::Ordinary)
                            .map_err(ValueTreeError::Memory)?;
                        let key =
                            oasis7_world_state::world_rules::emergency_observation_snapshot_key_v1(
                                region, window,
                            );
                        if directory.emergency().observation_present(&key) {
                            continue;
                        }
                        let guard = ctx.transaction.guard();
                        let registry = read_current_observation_registry_v2(
                            region,
                            window,
                            sources.rules.requests(),
                            sources.factory.roots,
                            state,
                            ctx,
                            &mut work.fixed_scan,
                            &mut work.composite_read,
                            &guard,
                            live,
                        )?;
                        if registry.registered().is_some() {
                            continue;
                        }
                        // The operator is the first original outer-order row for this region,
                        // even when that first row itself was not the eligibility witness.
                        for row in directory.emergency().minimum_services() {
                            tail.charge(row.region_id.len().min(region.len()))?;
                            if row.region_id == region {
                                seeds.push(
                                    &row.operator_agent_id,
                                    LifecycleRequestV1::Observe { region, now },
                                    tail,
                                )?;
                                break;
                            }
                        }
                    }
                }
                current.eligibility.orders.validate_current(ctx,&ctx.transaction.guard(),live).map_err(crate::world::incremental_industrial_eligibility::IndustrialEligibilityErrorV1::from)?;
                for (id, index) in current.eligibility.orders.iter_order_indices() {
                    let terminal = matches!(
                        index.phase(),
                        AgentIndustrialPhaseV1::Delivered
                            | AgentIndustrialPhaseV1::Cancelled
                            | AgentIndustrialPhaseV1::Disposed
                    );
                    // refund_pending is not part of the small directory index, so every
                    // terminal row is point-read; it must not silently disappear from work.
                    let guard = ctx.transaction.guard();
                    let order=oasis7_world_state::incremental_world_rule_composite_family::read_current_agent_order_v1(current.eligibility.orders.source(),id,state,ctx,&mut work.fixed_scan,&mut work.composite_read,&guard,live)?.ok_or(BodyMutationError::Invalid("lifecycle selected order absent"))?;
                    if terminal && !order.value().refund_pending {
                        continue;
                    }
                    let choice = if order.value().phase == AgentIndustrialPhaseV1::AwaitingDelivery
                        && delivery::current_lifecycle_order_delivery_blocker_v1(
                            id, current, state, ctx, parent, work, tail, now, live,
                        )?
                        .blocker
                        .is_none()
                    {
                        OrderLifecycleChoiceV1::Deliver
                    } else {
                        OrderLifecycleChoiceV1::Advance
                    };
                    seeds.push(
                        &order.value().owner_id,
                        LifecycleRequestV1::Order {
                            order: &order.value().order_id,
                            now,
                            command: choice,
                        },
                        tail,
                    )?;
                }
                for row in directory.emergency().authorizations() {
                    tail.charge_record()?;
                    if !row.active || row.starts_at > now {
                        continue;
                    }
                    let guard = ctx.transaction.guard();
                    // Rebind only the data scope after preceding read-set additions. No
                    // domain writes occurred, and work selection still uses the one snapshot.
                    let source=oasis7_world_state::incremental_world_rules_family::bind_current_emergency_lifecycle_v1(sources.rules.emergency(),sources.factory.roots,state,ctx,&mut work.fixed_scan,&mut work.composite_read,&guard,live)?;
                    let guard = ctx.transaction.guard();
                    let view=oasis7_world_state::incremental_world_rules_family::project_current_emergency_lifecycle_v1(&source,sources.rules.emergency(),sources.factory.roots,state,ctx,&mut work.fixed_scan,&mut work.composite_read,&guard,live)?;
                    let emergency = emergency_close::EmergencyCloseSourcesV1 {
                        delivery: current,
                        rules: sources.rules,
                    };
                    if emergency_close::validate_current_active_v1(
                        &row.id, &view, &source, &emergency, state, ctx, parent, work, extra,
                        world, tail, now, live,
                    )?
                    .is_err()
                    {
                        seeds.push(
                            &row.provider_id,
                            LifecycleRequestV1::EmergencyClose(&row.id),
                            tail,
                        )?;
                    }
                }
                Ok(seeds)
            })();
            let debit = (|| {
                let steps = before
                    .0
                    .checked_sub(world.outer_records_remaining)
                    .and_then(|n| {
                        before
                            .1
                            .checked_sub(world.remainder.cells_remaining)
                            .and_then(|m| n.checked_add(m))
                    })
                    .ok_or(BodyMutationError::Invalid(
                        "lifecycle initial counter increased",
                    ))?;
                tail.debit(steps, 0)
            })();
            if let Err(accounting) = debit {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                return match result {
                    Err(original) => Err(original),
                    Ok(_) => Err(accounting.into()),
                };
            }
            result
        },
    )
}
