//! Genuine Current source for the complete original starter feasibility tuple.
use super::*;
use crate::world::event_processing::factory_schedule_fixed::{
    FactoryBuildFactsV1, FactoryScheduleRecordV1,
};
use current::{CurrentFactoryScheduleFactsV1, EvaluationErrorV1};
use oasis7_world_state::{
    STARTER_ASSEMBLER_FACTORY_ID, STARTER_SMELTER_FACTORY_ID, STARTER_SMELTER_RECIPE_ID,
    StarterIndustrialFactsV1,
    incremental_factory_schedule_factory::recover_current_schedule_complete_factory_v1,
    incremental_factory_schedule_receipts::observe_current_factory_schedule_receipts_v1,
    incremental_inventory::recover_current_inventory_ledger_v1,
    incremental_world_native::recover_current_factory_industry_progress_v1,
    starter_industrial_feasibility_from_facts_v1,
};

pub(super) fn read_current_starter_blocker_v1<M, P, R, V, E, C, PE, PC>(
    source: &mut CurrentFactoryScheduleFactsV1<
        '_,
        '_,
        '_,
        '_,
        '_,
        '_,
        '_,
        M,
        P,
        R,
        V,
        E,
        C,
        PE,
        PC,
    >,
) -> Result<Option<String>, EvaluationErrorV1>
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
    source.charge(0)?;
    let factory = source.complete_factory(STARTER_SMELTER_FACTORY_ID)?;
    let smelter_profile = FactoryBuildFactsV1::factory_profile(source, STARTER_SMELTER_FACTORY_ID)?;
    let assembler_profile =
        FactoryBuildFactsV1::factory_profile(source, STARTER_ASSEMBLER_FACTORY_ID)?;
    let recipe=crate::world::event_processing::factory_schedule_fixed::FactoryRecipeFactsV1::recipe_profile(source,STARTER_SMELTER_RECIPE_ID)?;
    let site = match &factory {
        Some(factory) => FactoryBuildFactsV1::site(source, &factory.site_id)?,
        None => None,
    };
    let smelter_power = FactoryBuildFactsV1::power_profile(source, STARTER_SMELTER_FACTORY_ID)?;
    let assembler_power = FactoryBuildFactsV1::power_profile(source, STARTER_ASSEMBLER_FACTORY_ID)?;
    let output = match &factory {
        Some(factory) => Some(source.inventory_ledger(&factory.output_ledger)?),
        None => None,
    };
    let progress = source.progress()?;
    let progress_value = progress.value().ok_or_else(invalid)?;
    let receipts = source.receipt_rows()?;
    receipts.validate(
        source.context.transaction,
        source.common.memory(),
        source.live,
    )?;
    let facts = StarterIndustrialFactsV1 {
        time: source.now,
        smelter_factory: factory.as_deref(),
        smelter_profile: smelter_profile.as_deref(),
        assembler_profile: assembler_profile.as_deref(),
        smelter_recipe: recipe.as_deref(),
        smelter_site: site.as_deref(),
        smelter_power: smelter_power.as_deref(),
        assembler_power: assembler_power.as_deref(),
        smelter_output: output.as_ref().and_then(|v| v.as_ref()),
        milestone: progress_value.starter_industrial_milestone.as_ref(),
    };
    // The first fee-only callback is the fixed kernel's documented complete
    // result/formatting layout inventory. Subsequent callbacks stream original
    // tuple JSON into SHA256 and charge actual receipt comparisons, with no
    // retained wire or replacement hash implementation in the caller.
    let mut output_fee = true;
    let result = starter_industrial_feasibility_from_facts_v1(facts, receipts.facts(), &mut |n| {
        if output_fee {
            output_fee = false;
            source.fees.hold(n, source.common)
        } else {
            charge(source.common, n)
        }
    })?;
    let mut n=" [profile= revision= authority_snapshot= completion_boundary= evidence= progression_effect= next_action= next_recheck=]".len().checked_add(2*20).ok_or_else(limited)?;
    for value in [
        &result.profile_id,
        &result.authority_snapshot,
        &result.completion_boundary,
        &result.evidence_class,
        &result.progression_effect,
        &result.next_action,
    ] {
        n = n.checked_add(value.len()).ok_or_else(limited)?;
    }
    if let Some(value) = &result.blocker {
        n = n.checked_add(value.len()).ok_or_else(limited)?;
    }
    source
        .fees
        .hold(n.checked_mul(2).ok_or_else(limited)?, source.common)?;
    source
        .context
        .transaction
        .validate_guard(&source.context.transaction.guard(), source.live)
        .map_err(BodyMutationError::from)?;
    Ok(result.disabled_reason())
}
