use super::*;
pub(super) fn run<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    roots: &RootSetV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    outer_scan: &mut StateScanBudget,
    inner_scan: &mut StateScanBudget,
    work: &mut WorldNativeCompleteWorkV1,
) -> Result<ReservedValue<WorldNativeCompleteQualificationV1>, WorldNativeCompleteErrorV1> {
    let charge = ctx
        .values
        .memory
        .reserve(CARRIER, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    check(ctx)?;
    let primitive = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        primitive::qualify_world_primitive_fields_v1(
            roots,
            &mut local,
            &mut *outer_scan,
            &mut work.primitive,
        )?
    };
    check(ctx)?;
    let native = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        native::qualify_world_native_fields_v1(
            roots,
            &mut local,
            &mut *outer_scan,
            &mut work.native,
        )?
    };
    check(ctx)?;
    let inventory = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        inventory::qualify_inventory_family_v1(
            roots,
            &mut local,
            &mut *outer_scan,
            &mut work.inventory,
        )?
    };
    check(ctx)?;
    let factory = {
        let mut local = factory::FactoryFamilyReadContextV1 {
            state: ctx.state,
            values: ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            max_payload_bytes: work.max_payload_bytes,
            outer_scan: &mut *outer_scan,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut ctx.cancelled,
        };
        factory::qualify_factory_family_v1(roots, &mut local, &mut work.factory)?
    };
    check(ctx)?;
    let industry = {
        let mut local = industry::IndustryFamilyReadContextV1 {
            state: ctx.state,
            values: ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            max_payload_bytes: work.max_payload_bytes,
            outer_scan: &mut *outer_scan,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut ctx.cancelled,
        };
        industry::qualify_industry_family_v1(roots, &mut local, &mut work.industry)?
    };
    check(ctx)?;
    let market = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        market::qualify_module_marketplace_family_v1(
            roots,
            &mut local,
            &mut *outer_scan,
            &mut work.market,
        )?
    };
    check(ctx)?;
    let fixed = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        fixed::qualify_fixed_economy_family_v1(
            roots,
            &mut local,
            &mut *outer_scan,
            &mut *inner_scan,
            &mut work.legacy_cells_remaining,
        )?
    };
    check(ctx)?;
    let reward = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        reward::qualify_reward_family_v1(
            roots,
            &mut local,
            &mut *outer_scan,
            &mut *inner_scan,
            &mut work.legacy_cells_remaining,
            work.max_payload_bytes,
        )?
    };
    check(ctx)?;
    let main_policy = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        main_policy::qualify_main_token_policy_family_v1(
            roots,
            &mut local,
            &mut *outer_scan,
            &mut *inner_scan,
            &mut work.legacy_cells_remaining,
            work.max_payload_bytes,
        )?
    };
    check(ctx)?;
    let factory_authority = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        factory_authority::qualify_factory_authority_family_v1(
            roots,
            &mut local,
            &mut *outer_scan,
            &mut *inner_scan,
            &mut work.legacy_cells_remaining,
            work.max_payload_bytes,
        )?
    };
    check(ctx)?;
    let agent_authority = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        agent_authority::qualify_agent_authority_family_v1(
            roots,
            &mut local,
            &mut *outer_scan,
            &mut *inner_scan,
            &mut work.legacy_cells_remaining,
            work.max_payload_bytes,
        )?
    };
    check(ctx)?;
    let material = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        material::qualify_material_family_v1(
            roots,
            &mut local,
            &mut *outer_scan,
            &mut *inner_scan,
            &mut work.legacy_cells_remaining,
            work.max_payload_bytes,
        )?
    };
    check(ctx)?;
    let governance = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        governance::qualify_governance_family_v1(
            roots,
            &mut local,
            &mut *outer_scan,
            &mut *inner_scan,
            &mut work.legacy_cells_remaining,
            work.max_payload_bytes,
        )?
    };
    check(ctx)?;
    let logistics = {
        let mut local = logistics::LogisticsFamilyReadContextV1 {
            state: ctx.state,
            values: ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: ctx.values.class,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            max_payload_bytes: work.max_payload_bytes,
            outer_scan: &mut *outer_scan,
            take_state_error: &mut ctx.take_state_error,
            cancelled: &mut ctx.cancelled,
        };
        logistics::qualify_logistics_family_v1(roots, &mut local, &mut work.logistics)?
    };
    check(ctx)?;
    let diplomacy = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        diplomacy::qualify_diplomacy_family_v1(
            roots,
            &mut local,
            &mut *outer_scan,
            &mut *inner_scan,
            &mut work.legacy_cells_remaining,
            work.max_payload_bytes,
        )?
    };
    check(ctx)?;
    let rules = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Authorization));
        rules::qualify_world_rules_v1(
            roots,
            &mut local,
            &mut *outer_scan,
            &mut *inner_scan,
            &mut work.rules,
        )?
    };
    check(ctx)?;
    let controls = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        controls::qualify_world_collection_controls_v1(
            roots,
            &mut local,
            &mut *outer_scan,
            &mut work.controls,
        )?
    };
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
