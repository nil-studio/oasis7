//! Canonical native plans for the eleven built-in production recipes.
//! External callers select an exact recipe ID and batch count, never a plan.
//! Trusted and module-backed custom plans retain their separate execution paths.
use oasis7_wasm_abi::{MaterialStack, RecipeExecutionPlan};
fn literal_stacks<E>(
    stacks: &[(&str, i64)],
    work: &mut impl FnMut(u64, usize, usize) -> Result<(), E>,
) -> Result<Vec<MaterialStack>, E> {
    work(1, 0, stacks.len() * std::mem::size_of::<MaterialStack>())?;
    let mut out = Vec::with_capacity(stacks.len());
    for (kind, amount) in stacks {
        work(1, kind.len(), kind.len())?;
        out.push(MaterialStack::new((*kind).to_owned(), *amount));
    }
    Ok(out)
}
fn scale_material_stacks<E>(
    stacks: &[MaterialStack],
    scale: i64,
    work: &mut impl FnMut(u64, usize, usize) -> Result<(), E>,
) -> Result<Option<Vec<MaterialStack>>, E> {
    work(1, 0, stacks.len() * std::mem::size_of::<MaterialStack>())?;
    let mut out = Vec::with_capacity(stacks.len());
    for stack in stacks {
        work(1, stack.kind.len(), stack.kind.len())?;
        let kind = stack.kind.clone();
        let Some(amount) = stack.amount.checked_mul(scale) else {
            return Ok(None);
        };
        out.push(MaterialStack::new(kind, amount));
    }
    Ok(Some(out))
}
/// Same original plan constructor; the callback admits actual work and heap
/// before each allocation. It cannot alter any recipe or plan field.
pub fn builtin_recipe_plan_with_work_v1<E>(
    recipe_id: &str,
    accepted_batches: u32,
    work: &mut impl FnMut(u64, usize, usize) -> Result<(), E>,
) -> Result<Option<RecipeExecutionPlan>, E> {
    macro_rules! some {
        ($value:expr) => {
            match $value {
                Some(value) => value,
                None => return Ok(None),
            }
        };
    }
    work(1, recipe_id.len(), 0)?;
    if accepted_batches == 0 {
        return Ok(None);
    }
    let accepted_batches_i64 = i64::from(accepted_batches);
    let (consume, produce, byproducts, power_per_batch): (
        Vec<MaterialStack>,
        Vec<MaterialStack>,
        Vec<MaterialStack>,
        i64,
    ) = match recipe_id {
        "recipe.smelter.iron_ingot" => (
            literal_stacks(&[("iron_ore", 4), ("carbon_fuel", 1)], work)?,
            literal_stacks(&[("iron_ingot", 3)], work)?,
            literal_stacks(&[("slag", 1)], work)?,
            8,
        ),
        "recipe.smelter.copper_wire" => (
            literal_stacks(&[("copper_ore", 3)], work)?,
            literal_stacks(&[("copper_wire", 4)], work)?,
            literal_stacks(&[], work)?,
            6,
        ),
        "recipe.smelter.polymer_resin" => (
            literal_stacks(&[("carbon_fuel", 2), ("silicate_ore", 2)], work)?,
            literal_stacks(&[("polymer_resin", 2)], work)?,
            literal_stacks(&[("waste_resin", 1)], work)?,
            7,
        ),
        "recipe.smelter.alloy_plate" => (
            literal_stacks(&[("iron_ingot", 2), ("copper_wire", 2)], work)?,
            literal_stacks(&[("alloy_plate", 2)], work)?,
            literal_stacks(&[("slag", 1)], work)?,
            9,
        ),
        "recipe.assembler.gear" => (
            literal_stacks(&[("iron_ingot", 2)], work)?,
            literal_stacks(&[("gear", 1)], work)?,
            literal_stacks(&[], work)?,
            4,
        ),
        "recipe.assembler.control_chip" => (
            literal_stacks(&[("copper_wire", 4), ("polymer_resin", 2)], work)?,
            literal_stacks(&[("control_chip", 1)], work)?,
            literal_stacks(&[("waste_resin", 1)], work)?,
            6,
        ),
        "recipe.assembler.motor_mk1" => (
            literal_stacks(&[("gear", 2), ("copper_wire", 3)], work)?,
            literal_stacks(&[("motor_mk1", 1)], work)?,
            literal_stacks(&[], work)?,
            7,
        ),
        "recipe.assembler.logistics_drone" => (
            literal_stacks(
                &[("motor_mk1", 2), ("control_chip", 1), ("iron_ingot", 2)],
                work,
            )?,
            literal_stacks(&[("logistics_drone", 1)], work)?,
            literal_stacks(&[("assembly_scrap", 1)], work)?,
            12,
        ),
        "recipe.assembler.sensor_pack" => (
            literal_stacks(&[("control_chip", 1), ("copper_wire", 2)], work)?,
            literal_stacks(&[("sensor_pack", 1)], work)?,
            literal_stacks(&[("calibration_scrap", 1)], work)?,
            8,
        ),
        "recipe.assembler.module_rack" => (
            literal_stacks(&[("sensor_pack", 2), ("control_chip", 1)], work)?,
            literal_stacks(&[("module_rack", 1)], work)?,
            literal_stacks(&[("precision_scrap", 1)], work)?,
            10,
        ),
        "recipe.assembler.factory_core" => (
            literal_stacks(&[("module_rack", 1), ("alloy_plate", 3)], work)?,
            literal_stacks(&[("factory_core", 1)], work)?,
            literal_stacks(&[("structural_waste", 1)], work)?,
            14,
        ),
        _ => return Ok(None),
    };

    work(
        1,
        std::mem::size_of::<RecipeExecutionPlan>(),
        std::mem::size_of::<RecipeExecutionPlan>(),
    )?;
    Ok(Some(RecipeExecutionPlan::accepted(
        accepted_batches,
        some!(scale_material_stacks(&consume, accepted_batches_i64, work)?),
        some!(scale_material_stacks(&produce, accepted_batches_i64, work)?),
        some!(scale_material_stacks(
            &byproducts,
            accepted_batches_i64,
            work
        )?),
        some!(power_per_batch.checked_mul(accepted_batches_i64)),
        some!(super::canonical_recipe_base_duration_ticks(recipe_id)),
    )))
}

/// Returns the complete built-in plan for an exact canonical ID and positive batches.
pub fn builtin_recipe_plan(recipe_id: &str, accepted_batches: u32) -> Option<RecipeExecutionPlan> {
    match builtin_recipe_plan_with_work_v1(recipe_id, accepted_batches, &mut |_, _, _| {
        Ok::<_, std::convert::Infallible>(())
    }) {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Compare with the separately compiled built-in WASM recipe definitions.
    // These are evidence inputs only, not executable module code.
    const MODULE_SOURCES: [&str; 11] = [
        include_str!(
            "../../../oasis7_builtin_wasm_modules/m4_recipe_assembler_control_chip/src/lib.rs"
        ),
        include_str!(
            "../../../oasis7_builtin_wasm_modules/m4_recipe_assembler_factory_core/src/lib.rs"
        ),
        include_str!("../../../oasis7_builtin_wasm_modules/m4_recipe_assembler_gear/src/lib.rs"),
        include_str!(
            "../../../oasis7_builtin_wasm_modules/m4_recipe_assembler_logistics_drone/src/lib.rs"
        ),
        include_str!(
            "../../../oasis7_builtin_wasm_modules/m4_recipe_assembler_module_rack/src/lib.rs"
        ),
        include_str!(
            "../../../oasis7_builtin_wasm_modules/m4_recipe_assembler_motor_mk1/src/lib.rs"
        ),
        include_str!(
            "../../../oasis7_builtin_wasm_modules/m4_recipe_assembler_sensor_pack/src/lib.rs"
        ),
        include_str!(
            "../../../oasis7_builtin_wasm_modules/m4_recipe_smelter_alloy_plate/src/lib.rs"
        ),
        include_str!(
            "../../../oasis7_builtin_wasm_modules/m4_recipe_smelter_copper_wire/src/lib.rs"
        ),
        include_str!(
            "../../../oasis7_builtin_wasm_modules/m4_recipe_smelter_iron_ingot/src/lib.rs"
        ),
        include_str!(
            "../../../oasis7_builtin_wasm_modules/m4_recipe_smelter_polymer_resin/src/lib.rs"
        ),
    ];

    fn constant<'a>(source: &'a str, name: &str) -> &'a str {
        source
            .split_once(&format!("const {name}:"))
            .expect("built-in module constant")
            .1
            .split_once('=')
            .unwrap()
            .1
            .split_once(';')
            .unwrap()
            .0
            .trim()
    }
    fn module_stacks(source: &str, name: &str, batches: u32) -> Vec<MaterialStack> {
        let json = constant(source, name)
            .trim_start_matches('&')
            .replace('(', "[")
            .replace(')', "]");
        let entries: Vec<(String, i64)> = serde_json::from_str(&json).unwrap();
        entries
            .into_iter()
            .map(|(kind, amount)| MaterialStack::new(kind, amount * i64::from(batches)))
            .collect()
    }

    #[test]
    fn builtin_recipe_plans_match_all_eleven_wasm_definitions() {
        let mut unique = std::collections::BTreeSet::new();
        for source in MODULE_SOURCES {
            let id = constant(source, "RECIPE_ID").trim_matches('"');
            assert!(unique.insert(id), "duplicate built-in module recipe");
            for batches in [1, 7, u32::MAX] {
                let expected = RecipeExecutionPlan::accepted(
                    batches,
                    module_stacks(source, "CONSUME_PER_BATCH", batches),
                    module_stacks(source, "PRODUCE_PER_BATCH", batches),
                    module_stacks(source, "BYPRODUCTS_PER_BATCH", batches),
                    constant(source, "POWER_PER_BATCH").parse::<i64>().unwrap()
                        * i64::from(batches),
                    constant(source, "DURATION_TICKS").parse::<u32>().unwrap(),
                );
                let actual = builtin_recipe_plan(id, batches).expect("canonical built-in ID");
                assert_eq!(
                    actual, expected,
                    "complete canonical plan {id}, batches={batches}"
                );
            }
            assert!(builtin_recipe_plan(id, 0).is_none());
        }
        assert_eq!(unique.len(), 11);
    }

    #[test]
    fn builtin_recipe_catalog_rejects_aliases_and_noncanonical_ids() {
        for id in [
            "recipe.iron_ingot",
            "RECIPE.SMELTER.IRON_INGOT",
            " recipe.smelter.iron_ingot",
            "recipe.smelter.iron_ingot ",
            "recipe.custom",
            "",
        ] {
            assert!(
                builtin_recipe_plan(id, 1).is_none(),
                "unsupported ID {id:?}"
            );
        }
    }
}
