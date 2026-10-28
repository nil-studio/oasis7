//! Read-only bounds for exact, reviewed standard product validator code.
use super::World;
use crate::{RejectReason, WorldError};
use oasis7_wasm_abi::RecipeExecutionPlan;

// Code facts, not a configurable product catalog. A new digest is unknown until
// its actual validator is reviewed and boundary-tested. Never follow a mutable
// release manifest here: a same-name replacement may implement another limit.
const KNOWN_PRODUCT_LIMITS: &[(&str, &str, &str, i64)] = &[
    (
        "iron_ingot",
        "m4.product.material.iron_ingot",
        "7bd4f86542f307c03a6a2475fdc2adf904012e70cde0d21942393f0669379959",
        500,
    ),
    (
        "alloy_plate",
        "m4.product.material.alloy_plate",
        "727dae74e59814a960253a9467867df571ff89190e504b800b14835a6cacc2f4",
        400,
    ),
    (
        "control_chip",
        "m4.product.component.control_chip",
        "9d29b6c7d7514563060224d8a8dca167a0781be3cb7a126180821bcd3cfd3b03",
        200,
    ),
    (
        "motor_mk1",
        "m4.product.component.motor_mk1",
        "5f214f4e8b497b184b6a703953a6677bc271c366a2600c8cc40d6b5e91243a8d",
        128,
    ),
    (
        "logistics_drone",
        "m4.product.finished.logistics_drone",
        "13d31001e1b3d7c4fecc1e98704dc0f0fab104e5cd82544a06846387b8e12305",
        32,
    ),
    (
        "sensor_pack",
        "m4.product.component.sensor_pack",
        "7d9cb7f40beecf4e38f71e5868ac3e45385b7d96de95d26adea633fd3329b003",
        96,
    ),
    (
        "module_rack",
        "m4.product.finished.module_rack",
        "44e5f6441a69c64bd0b1720ceeba30fbffe540796a403eb1a96040cf8d359443",
        48,
    ),
    (
        "factory_core",
        "m4.product.infrastructure.factory_core",
        "795cb96379ec756148a540a071abf4dd825ba1d971adec6dd00957b4b92bf152",
        24,
    ),
];

impl World {
    pub(super) fn recipe_output_stack_limit_rejection(
        &self,
        plan: &RecipeExecutionPlan,
    ) -> Result<Option<RejectReason>, WorldError> {
        for (label, stacks) in [("produce", &plan.produce), ("byproduct", &plan.byproducts)] {
            for stack in stacks {
                // Use the completion selector, including its builtin preference.
                let Some(module_id) = self.resolve_product_module_for_stack(&stack.kind) else {
                    continue;
                };
                let manifest = self.active_module_manifest(&module_id)?;
                let Some((_, _, _, limit)) =
                    KNOWN_PRODUCT_LIMITS.iter().find(|(kind, id, hash, _)| {
                        *kind == stack.kind && *id == module_id && *hash == manifest.wasm_hash
                    })
                else {
                    continue;
                };
                if stack.amount > *limit {
                    return Ok(Some(RejectReason::RuleDenied {
                        notes: vec![format!(
                            "recipe {label} stack limit exceeded: kind={} amount={} limit={limit}; reduce batches and request a new quote",
                            stack.kind, stack.amount,
                        )],
                    }));
                }
            }
        }
        Ok(None)
    }
}
