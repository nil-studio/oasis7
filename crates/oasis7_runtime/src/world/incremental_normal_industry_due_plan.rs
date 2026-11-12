//! Paid call to the sole finite native built-in recipe catalogue. It produces
//! original plan data; custom/module plans retain their original separate lane.
use super::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::MemoryClass;
use std::mem::size_of;
pub(super) fn original_builtin_plan_v1(
    root: &oasis7_agent_api::world_action::RootOperationV1,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<Option<ReservedValue<oasis7_wasm_abi::RecipeExecutionPlan>>, BodyMutationError> {
    let limit = || {
        BodyMutationError::from(ValueTreeError::ResourceLimited(
            "industrial original builtin plan",
        ))
    };
    // recipe_catalog.rs has eleven exact IDs and duration has the same finite
    // eleven-case selection: at most 22 input-string comparisons. Each plan's
    // original vectors have <=3 consume +1 produce +1 byproduct slots and each
    // material literal <=17 bytes (calibration_scrap). scale_material_stacks
    // holds these 5 originals while collecting three vectors, each with the
    // locked Vec MIN_NON_ZERO_CAP=4 for MaterialStack: 5+3*4=17 live slots.
    // At most 5 originals +5 copied String payloads coexist. Three local Vec
    // headers coexist with the final plan; reject_reason is always None.
    let heap = 17usize
        .checked_mul(size_of::<crate::MaterialStack>())
        .and_then(|n| n.checked_add(10 * 17))
        .and_then(|n| n.checked_add(3 * size_of::<Vec<crate::MaterialStack>>()))
        .and_then(|n| {
            n.checked_add(size_of::<ReservedValue<oasis7_wasm_abi::RecipeExecutionPlan>>())
        })
        .ok_or_else(limit)?;
    let bytes = root
        .pinned_recipe_id
        .len()
        .checked_mul(22)
        .and_then(|n| n.checked_add(heap))
        .ok_or_else(limit)?;
    common
        .charge(22 + 10, u64::try_from(bytes).map_err(|_| limit())?)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let fee = common
        .memory()
        .reserve(heap, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    Ok(
        oasis7_world_state::builtin_recipe_plan(&root.pinned_recipe_id, root.accepted_batches)
            .map(|plan| fee.retain(plan)),
    )
}
