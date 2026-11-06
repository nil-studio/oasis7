use super::*;
pub(super) fn run<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<WorldNativeCompleteQualificationV1>,
    witness: &VerifiedWorldTransitionV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    inner_scan: &mut StateScanBudget,
    work: &mut WorldNativeCompleteWorkV1,
) -> Result<ReservedValue<WorldNativeCompleteQualificationV1>, WorldNativeCompleteErrorV1> {
    let charge = ctx
        .values
        .memory
        .reserve(CARRIER, ctx.values.class)
        .map_err(ValueTreeError::Memory)?;
    let roots = &witness.record().after;
    check(ctx)?;
    let primitive = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        primitive::accept_world_primitive_verified_v1(
            &parent.primitive,
            witness,
            &mut local,
            &mut work.primitive,
        )?
    };
    check(ctx)?;
    let native = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        native::accept_world_native_fields_verified_v1(
            &parent.native,
            witness,
            &mut local,
            &mut *inner_scan,
            &mut work.native,
        )?
    };
    check(ctx)?;
    let inventory = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        inventory::accept_inventory_verified_v1(
            &parent.inventory,
            witness,
            &mut local,
            &mut work.inventory,
        )?
    };
    check(ctx)?;
    let factory = {
        let mut local = factory::FactoryTransitionReadContextV1 {
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
            cancelled: &mut ctx.cancelled,
        };
        factory::accept_factory_verified_v1(
            witness,
            &parent.factory,
            &mut local,
            &mut work.factory,
        )?
    };
    check(ctx)?;
    let industry = {
        let mut local = industry::IndustryTransitionReadContextV1 {
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
            cancelled: &mut ctx.cancelled,
        };
        industry::accept_industry_verified_v1(
            witness,
            &parent.industry,
            &mut local,
            &mut work.industry,
        )?
    };
    check(ctx)?;
    let market = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        market::accept_module_marketplace_family_verified_v1(
            &parent.market,
            witness,
            &mut local,
            &mut *inner_scan,
            &mut work.market,
        )?
    };
    check(ctx)?;
    let fixed = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        fixed::accept_fixed_economy_verified_v1(
            &parent.fixed,
            witness,
            &mut local,
            &mut work.legacy_cells_remaining,
        )?
    };
    check(ctx)?;
    let reward = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        reward::accept_reward_verified_v1(
            &parent.reward,
            witness,
            &mut local,
            &mut *inner_scan,
            &mut work.legacy_cells_remaining,
            work.max_payload_bytes,
        )?
    };
    check(ctx)?;
    let main_policy = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        main_policy::accept_main_token_policy_verified_v1(
            &parent.main_policy,
            witness,
            &mut local,
            &mut *inner_scan,
            &mut work.legacy_cells_remaining,
            work.max_payload_bytes,
        )?
    };
    check(ctx)?;
    let factory_authority = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        factory_authority::accept_factory_authority_verified_v1(
            &parent.factory_authority,
            witness,
            &mut local,
            &mut *inner_scan,
            &mut work.legacy_cells_remaining,
            work.max_payload_bytes,
        )?
    };
    check(ctx)?;
    let agent_authority = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        agent_authority::accept_agent_authority_verified_v1(
            &parent.agent_authority,
            witness,
            &mut local,
            &mut *inner_scan,
            &mut work.legacy_cells_remaining,
            work.max_payload_bytes,
        )?
    };
    check(ctx)?;
    let material = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        material::accept_material_verified_v1(
            &parent.material,
            witness,
            &mut local,
            &mut *inner_scan,
            &mut work.legacy_cells_remaining,
            work.max_payload_bytes,
        )?
    };
    check(ctx)?;
    let governance = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        governance::accept_governance_verified_v1(
            &parent.governance,
            witness,
            &mut local,
            &mut *inner_scan,
            &mut work.legacy_cells_remaining,
            work.max_payload_bytes,
        )?
    };
    check(ctx)?;
    let logistics = {
        let mut local = logistics::LogisticsTransitionReadContextV1 {
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
            cancelled: &mut ctx.cancelled,
        };
        logistics::accept_logistics_verified_v1(
            witness,
            &parent.logistics,
            &mut local,
            &mut work.logistics,
        )?
    };
    check(ctx)?;
    let diplomacy = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        diplomacy::accept_diplomacy_verified_v1(
            &parent.diplomacy,
            witness,
            &mut local,
            &mut work.legacy_cells_remaining,
            work.max_payload_bytes,
        )?
    };
    check(ctx)?;
    let rules = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Authorization));
        rules::accept_world_rules_verified_v1(
            &parent.rules,
            witness,
            &mut local,
            &mut *inner_scan,
            &mut work.rules,
        )?
    };
    check(ctx)?;
    let controls = {
        let mut local = local!(ctx, roots.state_root(StateNamespaceV1::Business));
        controls::accept_world_collection_controls_verified_v1(
            &parent.controls,
            witness,
            &mut local,
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
