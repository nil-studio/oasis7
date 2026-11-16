//! Genuine parent holders and scoped fresh Current order sources for the sole
//! lifecycle driver. No cloned World, temporary meter, or self-referential
//! owner is constructed; the existing paid directory mapper is reused.
use super::delivery::{
    IndustrialDeliveryErrorV1, IndustrialDeliverySourcesV1, IndustrialDeliveryWorkV1,
};
use super::*;
use crate::world::{
    incremental_body::BodyMutationError,
    incremental_factory_directory_orders::{
        AcceptedFactoryOrderDirectoryV1, FactoryOrderDirectoryWorkV1,
        open_current_factory_order_directory_v1,
    },
    incremental_industrial_eligibility::IndustrialEligibilitySourcesV1,
    incremental_normal_action_factory_schedule::FactoryScheduleSourcesV1,
    incremental_normal_industry_due::owner_sources,
    incremental_world_event_counter::WorldJournalRetentionV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_fixed_economy_records::FixedEconomyFamilyQualificationV1,
    incremental_runtime_artifact_resources::RuntimeArtifactResourcesQualificationV1,
    incremental_runtime_governance::RuntimeGovernanceQualificationV1,
    incremental_runtime_revocation::RuntimeRevocationQualificationV1,
    incremental_world_native::IndustrialClaimDirectoryV1,
    incremental_world_rule_composite_family::bind_current_agent_orders_v1,
    incremental_world_rules_family::{WorldRulesFamilyWorkV1, WorldRulesQualificationV1},
};
pub(in crate::world) struct NormalWorldRuleLifecycleSourcesV1<'a> {
    pub rules: &'a ReservedValue<WorldRulesQualificationV1>,
    pub orders: &'a ReservedValue<AcceptedFactoryOrderDirectoryV1>,
    pub factory: &'a FactoryScheduleSourcesV1<'a>,
    pub governance: &'a ReservedValue<RuntimeGovernanceQualificationV1>,
    pub revocation: &'a ReservedValue<RuntimeRevocationQualificationV1>,
    pub artifacts: &'a ReservedValue<RuntimeArtifactResourcesQualificationV1>,
    pub claims: &'a ReservedValue<IndustrialClaimDirectoryV1>,
    pub fixed: &'a ReservedValue<FixedEconomyFamilyQualificationV1>,
    pub retention: &'a ReservedValue<WorldJournalRetentionV1>,
}
pub(in crate::world) struct NormalWorldRuleLifecycleWorkV1<'a> {
    pub delivery: &'a mut IndustrialDeliveryWorkV1,
    pub orders: &'a mut FactoryOrderDirectoryWorkV1,
    pub world: &'a mut WorldRulesFamilyWorkV1,
}
#[allow(clippy::too_many_arguments)]
pub(super) fn with_current_lifecycle_sources_v1<M, P, R, V, E, C, PE, PC, T>(
    sources: &NormalWorldRuleLifecycleSourcesV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    work: &mut IndustrialDeliveryWorkV1,
    orders: &mut FactoryOrderDirectoryWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
    consume: impl FnOnce(
        &IndustrialDeliverySourcesV1<'_, '_>,
        &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
        &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
        &mut IndustrialDeliveryWorkV1,
        &mut FormalBoundedContextV1<'_>,
    ) -> Result<T, IndustrialDeliveryErrorV1>,
) -> Result<T, IndustrialDeliveryErrorV1>
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
    let current = owner_sources::paid(
        ctx,
        &mut work.eligibility,
        orders,
        common,
        |ctx, work, _| {
            let guard = ctx.transaction.guard();
            Ok(bind_current_agent_orders_v1(
                sources.rules.orders(),
                sources.factory.roots,
                parent.state,
                ctx,
                &mut work.scan,
                &mut work.typed,
                &guard,
                live,
            )?)
        },
    )?;
    let directory = owner_sources::paid(
        ctx,
        &mut work.eligibility,
        orders,
        common,
        |ctx, work, orders| {
            let guard = ctx.transaction.guard();
            open_current_factory_order_directory_v1(
                sources.orders,
                &current,
                parent.state,
                ctx,
                &mut work.scan,
                &mut work.typed,
                orders,
                &guard,
                live,
            )
        },
    )?;
    let eligibility = IndustrialEligibilitySourcesV1 {
        roots: sources.factory.roots,
        orders: &directory,
        agents: sources.factory.lifecycle.agents,
        native: sources.factory.lifecycle.native,
        factories: sources.factory.lifecycle.factories,
        sites: sources.factory.factory_authority,
        anchors: sources.factory.agent_authority,
        revocation: sources.revocation,
        artifacts: sources.artifacts,
    };
    let current = IndustrialDeliverySourcesV1 {
        eligibility,
        claims: sources.claims,
        fixed: sources.fixed,
        orders_parent: sources.rules.orders(),
    };
    consume(&current, ctx, parent, work, common)
}
