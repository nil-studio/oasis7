use super::*;
pub(super) fn run<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    source: &ReservedValue<WorldNativeCompleteQualificationV1>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    work: &mut WorldNativeCompleteWorkV1,
) -> Result<ReservedValue<WorldNativeCompleteQualificationV1>, WorldNativeCompleteErrorV1> {
    let charge = ctx
        .values
        .memory
        .reserve(CARRIER, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    check(ctx)?;
    let primitive = primitive::readmit_world_primitive_fields_ordinary_v1(
        &source.primitive,
        roots,
        ctx.values.memory,
        &mut ctx.cancelled,
    )?;
    check(ctx)?;
    let native = native::readmit_world_native_fields_ordinary_v1(
        &source.native,
        roots,
        ctx.values.memory,
        &mut ctx.cancelled,
    )?;
    check(ctx)?;
    let inventory = inventory::readmit_inventory_family_ordinary_v1(
        &source.inventory,
        roots,
        ctx.values.memory,
        &mut work.inventory,
        &mut ctx.cancelled,
    )?;
    check(ctx)?;
    let factory = factory::readmit_factory_family_ordinary_v1(
        &source.factory,
        roots,
        ctx.values.memory,
        &mut ctx.cancelled,
    )?;
    check(ctx)?;
    let industry = industry::readmit_industry_family_ordinary_v1(
        &source.industry,
        roots,
        ctx.values.memory,
        &mut ctx.cancelled,
    )?;
    check(ctx)?;
    let market = market::readmit_module_marketplace_family_ordinary_v1(
        &source.market,
        roots,
        ctx.values.memory,
        &mut ctx.cancelled,
    )?;
    check(ctx)?;
    let fixed = fixed::readmit_fixed_economy_family_ordinary_v1(
        &source.fixed,
        roots,
        ctx.values.memory,
        &mut ctx.cancelled,
    )?;
    check(ctx)?;
    let reward = reward::readmit_reward_family_ordinary_v1(
        &source.reward,
        roots,
        ctx.values.memory,
        &mut ctx.cancelled,
    )?;
    check(ctx)?;
    let main_policy = main_policy::readmit_main_token_policy_family_ordinary_v1(
        &source.main_policy,
        roots,
        ctx.values.memory,
        &mut ctx.cancelled,
    )?;
    check(ctx)?;
    let factory_authority = factory_authority::readmit_factory_authority_family_ordinary_v1(
        &source.factory_authority,
        roots,
        ctx.values.memory,
        &mut ctx.cancelled,
    )?;
    check(ctx)?;
    let agent_authority = agent_authority::readmit_agent_authority_family_ordinary_v1(
        &source.agent_authority,
        roots,
        ctx.values.memory,
        &mut ctx.cancelled,
    )?;
    check(ctx)?;
    let material = material::readmit_material_family_ordinary_v1(
        &source.material,
        roots,
        ctx.values.memory,
        &mut ctx.cancelled,
    )?;
    check(ctx)?;
    let governance = governance::readmit_governance_family_ordinary_v1(
        &source.governance,
        roots,
        ctx.values.memory,
        &mut ctx.cancelled,
    )?;
    check(ctx)?;
    let logistics = logistics::readmit_logistics_family_ordinary_v1(
        &source.logistics,
        roots,
        ctx.values.memory,
        &mut ctx.cancelled,
    )?;
    check(ctx)?;
    let diplomacy =
        diplomacy::readmit_diplomacy_family_ordinary_v1(&source.diplomacy, ctx.values.memory)?;
    check(ctx)?;
    let rules = rules::readmit_world_rules_ordinary_v1(
        &source.rules,
        roots,
        ctx.values.memory,
        &mut ctx.cancelled,
    )?;
    check(ctx)?;
    let controls = controls::readmit_world_collection_controls_ordinary_v1(
        &source.controls,
        roots,
        ctx.values.memory,
        &mut work.controls,
        &mut ctx.cancelled,
    )?;
    check(ctx)?;
    let value = WorldNativeCompleteQualificationV1 {
        roots: roots.digest().map_err(WorldStateOwnerError::RootSet)?,
        class: ctx.values.class,
        primitive,
        native,
        inventory,
        factory,
        industry,
        market,
        fixed,
        reward,
        main_policy,
        factory_authority,
        agent_authority,
        material,
        governance,
        logistics,
        diplomacy,
        rules,
        controls,
    };
    value.validate_binding(roots)?;
    value.check_pool(ctx.values.memory)?;
    check(ctx)?;
    Ok(charge.retain(value))
}
