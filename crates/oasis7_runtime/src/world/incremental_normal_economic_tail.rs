//! One concrete normal-tail path for due economy, material transit and world
//! rules, in original step order. Later gameplay/organization/cognition phases
//! remain separate obligations; this path cannot mint a completed tick.
use super::*;
use crate::world::{
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_factory_directory_orders::FactoryOrderDirectoryWorkV1,
    incremental_factory_directory_records::{
        AcceptedFactoryRecipeDirectoryV1, FactoryRecipeDirectoryWorkV1,
    },
    incremental_industrial_eligibility::IndustrialEligibilityWorkV1,
    incremental_module_invocation::{
        AcceptedInvocationDirectoryV1, CurrentInvocationEnumerationWorkV1,
        RootModuleExecutionContextV1, RootModuleExecutionErrorV1,
    },
    incremental_normal_action_factory_schedule::FactoryScheduleWorkV1,
    incremental_normal_industry_due::{
        NormalIndustryDueSourcesV1, NormalIndustryDueWorkV1,
        delivery::{CurrentProductValidationDeliveryIndexV1, ProductValidationDeliveryWorkV1},
    },
    incremental_normal_world_rule_lifecycle::{
        NormalWorldRuleLifecycleSourcesV1, NormalWorldRuleLifecycleWorkV1,
        delivery::IndustrialDeliveryWorkV1, emergency_close::sources::EmergencyServiceSourceWorkV1,
    },
    incremental_normal_world_rule_observation_v2::ObservationJsonOriginalSinkV2,
};
use oasis7_formal_protocol::{
    incremental_material::OriginalObjectStreamReader,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_world_state::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_industry_jobs::IndustryFamilyReadContextV1,
    incremental_world_rules_family::WorldRulesFamilyWorkV1,
};

/// Borrow the original per-family counters once. The same mutable factory,
/// order, authority and logistics counters continue across all three phases.
pub(in crate::world) struct NormalEconomicTailWorkV1<'a> {
    pub factory: &'a mut FactoryScheduleWorkV1,
    pub terminal: &'a mut IndustrialEligibilityWorkV1,
    pub recipes: &'a mut FactoryRecipeDirectoryWorkV1,
    pub orders: &'a mut FactoryOrderDirectoryWorkV1,
    pub world_delivery: &'a mut IndustrialDeliveryWorkV1,
    pub world: &'a mut WorldRulesFamilyWorkV1,
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn execute_normal_economic_tail_v1<P, CE, CC, PE, PC, IE, IC, ME, S>(
        &mut self,
        state: &M,
        parent_source: &ReservedValue<BoundNormalTickParentV1<'_>>,
        sources: &NormalWorldRuleLifecycleSourcesV1<'_>,
        recipes: &ReservedValue<AcceptedFactoryRecipeDirectoryV1>,
        work: &mut NormalEconomicTailWorkV1<'_>,
        product_delivery: &mut CurrentProductValidationDeliveryIndexV1<'_>,
        product_work: &mut ProductValidationDeliveryWorkV1,
        industry_read: &mut IndustryFamilyReadContextV1<'_, M, R, IE, IC>,
        sink: &mut S,
        materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
        current: &mut WorldFieldReadContextV1<'_, P, R, CE, CC>,
        parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        normal: &mut NormalTailWorkV1<'_, '_>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<(), RootModuleExecutionErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        CE: FnMut() -> Option<ValueTreeError>,
        CC: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
        IE: FnMut() -> Option<ValueTreeError>,
        IC: FnMut() -> bool,
        ME: FnMut() -> Option<ValueTreeError>,
        S: ObservationJsonOriginalSinkV2,
        S::Error: std::fmt::Debug + Send + Sync + 'static,
    {
        let result = (|| {
            let live = live_owner();
            normal
                .validate_parent(parent_source)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let bound = NormalTailSourcesV1::from_bound_parent(parent_source);
            self.ctx
                .transaction
                .validate_guard(&self.ctx.transaction.guard(), &live)?;
            if self.ctx.transaction.parent_fence() != parent_source.fence()
                || !parent_source.uses_budget(normal.memory())
            {
                return Err(RootModuleExecutionErrorV1::Body(
                    BodyMutationError::Invalid("normal economic parent scope"),
                ));
            }
            normal
                .charge(std::mem::size_of_val(work) + std::mem::size_of_val(sources))
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let compared = crate::world::incremental_normal_sources::root_bytes(
                bound.roots(),
                sources.factory.roots,
            )
            .map_err(|e| {
                RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
            })?;
            normal
                .debit(1, compared)
                .map_err(RootModuleExecutionErrorV1::Body)?;
            if &**bound.roots() != sources.factory.roots {
                return Err(RootModuleExecutionErrorV1::Body(
                    BodyMutationError::Invalid("normal economic source roots"),
                ));
            }
            // Each phase routes its original batch before the next phase snapshots its
            // own Current inputs. No retained parent list substitutes for those reads.
            {
                let due = NormalIndustryDueSourcesV1 {
                    factory: sources.factory,
                    rules: sources.rules,
                    records: recipes,
                    orders: sources.orders,
                    retention: sources.retention,
                };
                let mut due_work = NormalIndustryDueWorkV1 {
                    factory: &mut *work.factory,
                    terminal: &mut *work.terminal,
                    records: &mut *work.recipes,
                    orders: &mut *work.orders,
                };
                self.execute_normal_industry_due_v1(
                    &due,
                    &mut due_work,
                    product_delivery,
                    product_work,
                    materials,
                    current,
                    parent,
                    normal,
                    directory,
                    enumeration,
                    live_owner,
                )?;
            }
            self.execute_normal_material_transits_v1(
                parent_source,
                sources.factory,
                &mut work.factory.lifecycle,
                &mut work.factory.logistics_write,
                industry_read,
                materials,
                current,
                parent,
                normal,
                directory,
                enumeration,
                live_owner,
            )?;
            {
                let mut world_work = NormalWorldRuleLifecycleWorkV1 {
                    delivery: &mut *work.world_delivery,
                    orders: &mut *work.orders,
                    world: &mut *work.world,
                };
                let mut authority = EmergencyServiceSourceWorkV1 {
                    authority: &mut work.factory.authority,
                    authority_cells: &mut work.factory.authority_cells,
                };
                self.execute_normal_world_rule_lifecycle_v1(
                    state,
                    sources,
                    &mut world_work,
                    parent_source,
                    &mut authority,
                    sink,
                    materials,
                    current,
                    parent,
                    normal,
                    directory,
                    enumeration,
                    live_owner,
                )?;
            }
            self.ctx
                .transaction
                .validate_guard(&self.ctx.transaction.guard(), &live_owner())?;
            Ok(())
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }
}
