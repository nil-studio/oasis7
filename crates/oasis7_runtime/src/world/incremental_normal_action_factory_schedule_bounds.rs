//! Explicit output/collection layout inventory for the fixed industrial kernels.
use super::*;
use oasis7_wasm_abi::FactoryModuleSpec;
use std::mem::{align_of, size_of};
const BUILD_LABELS: &[&str] = &[
    "factory_id cannot be empty",
    "factory identity is retired: {}",
    "factory already exists: {}",
    "recipe_slots must be > 0",
    "factory profile unknown: factory_id={}",
    "site_unknown: site is not registered: {site_id}",
    "builder location authority is unknown: builder_agent_id={builder_agent_id}",
    "site_access_or_location_blocked: site={} builder={} site_active={} chunk_ready={} site_location={} builder_location={} builder_location_active={} authorized={}",
    "construction power profile unknown: factory_id={}",
    "construction power profile inactive_or_stale: factory_id={} revision={} active={}",
    "factory build_cost",
];
const RECIPE_LABELS: &[&str] = &[
    "schedule recipe requester is not factory builder: factory_id={factory_id} requester={requester_agent_id} builder={}",
    "recipe plan rejected: {reason}",
    "consume",
    "produce",
    "byproduct",
    "recipe accepted_batches must be > 0",
    "recipe plan must produce at least one positive material output",
    "recipe stage gate denied: recipe={} required_stage={} current_stage={}",
    "recipe preferred_factory_tags mismatch: recipe={} preferred={:?} factory_tags={:?}",
    "product unlock_stage denied: product={} required_stage={} current_stage={}",
    "duplicate logistics route binding: route_id={route_id}",
    "logistics route binding count must equal distinct consumed material kinds: routes={} kinds={}",
    "logistics route not found: route_id={route_id}",
    "logistics route is not completed: route_id={route_id}",
    "logistics route destination mismatch: route_id={route_id}",
    "logistics route material kind is invalid: route_id={route_id}",
    "logistics route material mismatch: route_id={route_id}",
    "recipe consume",
    "recipe power_required must be >= 0",
];

fn inventory(labels: &[&str]) -> Option<usize> {
    labels
        .iter()
        .try_fold(0usize, |n, s| n.checked_add(s.len()))
}
pub(super) fn node<K, V>() -> Option<usize> {
    let a = align_of::<K>()
        .max(align_of::<V>())
        .max(align_of::<usize>());
    11usize
        .checked_mul(size_of::<K>().checked_add(size_of::<V>())?)
        .and_then(|n| n.checked_add(13 * size_of::<usize>() + 2 * size_of::<u16>()))
        .and_then(|n| n.checked_add(7 * (a - 1)))
}
pub(super) fn material_map_bytes(n: usize) -> Result<usize, BodyMutationError> {
    n.checked_mul(node::<String, i64>().ok_or_else(limited)?)
        .and_then(|n| n.checked_mul(2))
        .and_then(|n| n.checked_add(2 * size_of::<std::collections::BTreeMap<String, i64>>()))
        .ok_or_else(limited)
}
pub(super) fn tag_normalization_bytes(tags: &[String]) -> Result<usize, BodyMutationError> {
    // One actual normalization owns a set, then a Vec while the set iterator is
    // live. Two such values can overlap in the original comparison/Debug error.
    let mut n =
        oasis7_world_state::incremental_collection_layout::string_set_output_heap_v1(tags.len())?
            .checked_add(
                tags.len()
                    .checked_mul(size_of::<String>())
                    .ok_or_else(limited)?,
            )
            .ok_or_else(limited)?;
    for tag in tags {
        n = n.checked_add(tag.len()).ok_or_else(limited)?;
    }
    n.checked_mul(2).ok_or_else(limited)
}
pub(super) fn build_output_bytes(spec: &FactoryModuleSpec) -> Option<usize> {
    // Full inventory, including mutually exclusive notes, dominates each fixed
    // template. Eight numeric insertions use <=20 decimal bytes each. Two live
    // growable formatting buffers are charged independently from source payload.
    inventory(BUILD_LABELS)?
        .checked_add(8 * 20)?
        .checked_mul(4)?
        .checked_add(
            size_of::<crate::WorldEventBody>()
                + size_of::<crate::RejectReason>()
                + size_of::<Vec<String>>()
                + size_of::<String>(),
        )?
        .checked_add(tag_normalization_bytes(&spec.tags).ok()?)
}
pub(super) fn recipe_output_bytes() -> Option<usize> {
    inventory(RECIPE_LABELS)?
        .checked_add(8 * 20)?
        .checked_mul(4)?
        .checked_add(
            size_of::<crate::WorldEventBody>()
                + size_of::<crate::RejectReason>()
                + size_of::<Vec<String>>()
                + size_of::<String>(),
        )
}
