//! Sealed read-only observations for the sole original industrial predicates.
//! A recovered record is data; only a Selected/Accepted committer may write it.
use super::*;
use oasis7_proto::memory_budget::ReservedValue;
use std::ops::Deref;
pub(in crate::world) enum FactoryScheduleRecordV1<'a, T> {
    Legacy(&'a T),
    Current(ReservedValue<T>),
    Native(oasis7_world_state::incremental_world_native::WorldNativeRecoveredRecordV1<T>),
}
impl<T> Deref for FactoryScheduleRecordV1<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        match self {
            Self::Legacy(v) => v,
            Self::Current(v) => v,
            Self::Native(v) => v.value().value(),
        }
    }
}
pub(in crate::world) mod sealed {
    pub trait Sealed {}
}
pub(in crate::world) trait FactoryBuildFactsV1<'source>: sealed::Sealed {
    type Error: From<WorldError>;
    fn charge(&mut self, bytes: usize) -> Result<(), Self::Error>;
    fn reserve_action_output(
        &mut self,
        builder: &str,
        site: &str,
        spec: &oasis7_wasm_abi::FactoryModuleSpec,
    ) -> Result<(), Self::Error>;
    fn reserve_material_balances(&mut self, stacks: &[MaterialStack]) -> Result<(), Self::Error>;
    fn now(&self) -> u64;
    fn agent_exists(&mut self, id: &str) -> Result<bool, Self::Error>;
    fn retired(&mut self, id: &str) -> Result<bool, Self::Error>;
    fn factory_exists(&mut self, id: &str) -> Result<bool, Self::Error>;
    fn any_pending_factory(&mut self, id: &str) -> Result<bool, Self::Error>;
    fn starter_assembler_blocker(&mut self, id: &str) -> Result<Option<String>, Self::Error>;
    fn factory_profile(
        &mut self,
        id: &str,
    ) -> Result<Option<FactoryScheduleRecordV1<'source, crate::FactoryProfileV1>>, Self::Error>;
    fn site(
        &mut self,
        id: &str,
    ) -> Result<Option<FactoryScheduleRecordV1<'source, crate::FactorySiteAuthorityV1>>, Self::Error>;
    fn location(
        &mut self,
        id: &str,
    ) -> Result<
        Option<FactoryScheduleRecordV1<'source, crate::AgentLocationAuthorityV1>>,
        Self::Error,
    >;
    fn anchor_revision(&mut self, id: &str) -> Result<Result<u64, WorldError>, Self::Error>;
    fn power_profile(
        &mut self,
        id: &str,
    ) -> Result<
        Option<FactoryScheduleRecordV1<'source, crate::FactoryConstructionPowerProfileV1>>,
        Self::Error,
    >;
    fn electricity(&mut self, id: &str) -> Result<i64, Self::Error>;
    fn material(&mut self, ledger: &MaterialLedgerId, kind: &str) -> Result<i64, Self::Error>;
}
pub(in crate::world) struct LegacyFactoryBuildFactsV1<'a> {
    world: &'a World,
}
impl<'a> LegacyFactoryBuildFactsV1<'a> {
    pub(in crate::world) fn new(world: &'a World) -> Self {
        Self { world }
    }
}
impl sealed::Sealed for LegacyFactoryBuildFactsV1<'_> {}
impl<'a> FactoryBuildFactsV1<'a> for LegacyFactoryBuildFactsV1<'a> {
    type Error = WorldError;
    fn charge(&mut self, _: usize) -> Result<(), Self::Error> {
        Ok(())
    }
    fn reserve_action_output(
        &mut self,
        _: &str,
        _: &str,
        _: &oasis7_wasm_abi::FactoryModuleSpec,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    fn reserve_material_balances(&mut self, _: &[MaterialStack]) -> Result<(), Self::Error> {
        Ok(())
    }
    fn now(&self) -> u64 {
        self.world.state.time
    }
    fn agent_exists(&mut self, id: &str) -> Result<bool, Self::Error> {
        Ok(self.world.state.agents.contains_key(id))
    }
    fn retired(&mut self, id: &str) -> Result<bool, Self::Error> {
        Ok(self.world.state.retired_factory_ids.contains(id))
    }
    fn factory_exists(&mut self, id: &str) -> Result<bool, Self::Error> {
        Ok(self.world.state.factories.contains_key(id))
    }
    fn any_pending_factory(&mut self, id: &str) -> Result<bool, Self::Error> {
        Ok(self
            .world
            .state
            .pending_factory_builds
            .values()
            .any(|job| job.spec.factory_id == id))
    }
    fn starter_assembler_blocker(&mut self, id: &str) -> Result<Option<String>, Self::Error> {
        Ok(if id == crate::STARTER_ASSEMBLER_FACTORY_ID {
            self.world
                .state
                .starter_industrial_feasibility()
                .disabled_reason()
        } else {
            None
        })
    }
    fn factory_profile(
        &mut self,
        id: &str,
    ) -> Result<Option<FactoryScheduleRecordV1<'a, crate::FactoryProfileV1>>, Self::Error> {
        Ok(self
            .world
            .state
            .factory_profiles
            .get(id)
            .map(FactoryScheduleRecordV1::Legacy))
    }
    fn site(
        &mut self,
        id: &str,
    ) -> Result<Option<FactoryScheduleRecordV1<'a, crate::FactorySiteAuthorityV1>>, Self::Error>
    {
        Ok(self
            .world
            .state
            .factory_site_authorities
            .get(id)
            .map(FactoryScheduleRecordV1::Legacy))
    }
    fn location(
        &mut self,
        id: &str,
    ) -> Result<Option<FactoryScheduleRecordV1<'a, crate::AgentLocationAuthorityV1>>, Self::Error>
    {
        Ok(self
            .world
            .state
            .agent_location_authorities
            .get(id)
            .map(FactoryScheduleRecordV1::Legacy))
    }
    fn anchor_revision(&mut self, id: &str) -> Result<Result<u64, WorldError>, Self::Error> {
        Ok(self
            .world
            .state
            .active_location_anchor_revision(id, self.world.state.time))
    }
    fn power_profile(
        &mut self,
        id: &str,
    ) -> Result<
        Option<FactoryScheduleRecordV1<'a, crate::FactoryConstructionPowerProfileV1>>,
        Self::Error,
    > {
        Ok(self
            .world
            .state
            .factory_construction_power_profiles
            .get(id)
            .map(FactoryScheduleRecordV1::Legacy))
    }
    fn electricity(&mut self, id: &str) -> Result<i64, Self::Error> {
        Ok(self
            .world
            .state
            .agents
            .get(id)
            .map(|cell| {
                cell.state
                    .resources
                    .get(oasis7_agent_api::world_action::ResourceKind::Electricity)
            })
            .unwrap_or(0))
    }
    fn material(&mut self, ledger: &MaterialLedgerId, kind: &str) -> Result<i64, Self::Error> {
        Ok(self.world.ledger_material_balance(ledger, kind))
    }
}

pub(in crate::world) enum RecipeFactoryV1<'a> {
    Legacy(&'a oasis7_world_state::FactoryState),
    Current(
        ReservedValue<
            oasis7_world_state::incremental_factory_schedule_factory::FactoryScheduleFactoryV1,
        >,
    ),
}
impl RecipeFactoryV1<'_> {
    pub(in crate::world) fn builder(&self) -> &str {
        match self {
            Self::Legacy(v) => &v.builder_agent_id,
            Self::Current(v) => v.builder(),
        }
    }
    pub(in crate::world) fn slots(&self) -> u16 {
        match self {
            Self::Legacy(v) => v.spec.recipe_slots,
            Self::Current(v) => v.slots(),
        }
    }
    pub(in crate::world) fn tags(&self) -> &[String] {
        match self {
            Self::Legacy(v) => &v.spec.tags,
            Self::Current(v) => v.tags(),
        }
    }
    pub(in crate::world) fn input(&self) -> &MaterialLedgerId {
        match self {
            Self::Legacy(v) => &v.input_ledger,
            Self::Current(v) => v.input(),
        }
    }
    pub(in crate::world) fn output(&self) -> &MaterialLedgerId {
        match self {
            Self::Legacy(v) => &v.output_ledger,
            Self::Current(v) => v.output(),
        }
    }
}
pub(in crate::world) trait FactoryRecipeFactsV1<'source>: sealed::Sealed {
    type Error: From<WorldError>;
    fn charge(&mut self, bytes: usize) -> Result<(), Self::Error>;
    fn reserve_recipe_output(
        &mut self,
        requester: &str,
        factory: &str,
        recipe: &str,
        plan: &oasis7_wasm_abi::RecipeExecutionPlan,
        routes: &[String],
        paths: &[String],
    ) -> Result<(), Self::Error>;
    fn now(&self) -> u64;
    fn agent_exists(&mut self, id: &str) -> Result<bool, Self::Error>;
    fn factory(&mut self, id: &str) -> Result<Option<RecipeFactoryV1<'source>>, Self::Error>;
    fn pending_recipes_for_factory(&mut self, id: &str) -> Result<usize, Self::Error>;
    fn output_stack_rejection(
        &mut self,
        plan: &oasis7_wasm_abi::RecipeExecutionPlan,
    ) -> Result<Option<RejectReason>, Self::Error>;
    fn stage(&mut self) -> Result<crate::IndustryStage, Self::Error>;
    fn recipe_profile(
        &mut self,
        id: &str,
    ) -> Result<Option<FactoryScheduleRecordV1<'source, crate::RecipeProfileV1>>, Self::Error>;
    fn product_profile(
        &mut self,
        id: &str,
    ) -> Result<Option<FactoryScheduleRecordV1<'source, crate::ProductProfileV1>>, Self::Error>;
    fn effective_consume(
        &mut self,
        consume: &[MaterialStack],
        produce: &[MaterialStack],
    ) -> Result<Vec<MaterialStack>, Self::Error>;
    fn route(
        &mut self,
        id: &str,
    ) -> Result<
        Option<FactoryScheduleRecordV1<'source, oasis7_world_state::LogisticsRouteV1>>,
        Self::Error,
    >;
    fn route_completed(&mut self, id: &str) -> Result<bool, Self::Error>;
    fn path_id(&mut self, routes: &[String]) -> Result<Option<String>, Self::Error>;
    fn allocate_paths(
        &mut self,
        ledger: &MaterialLedgerId,
        routes: &[String],
        paths: &[String],
        consume: &[MaterialStack],
    ) -> Result<Result<(), String>, Self::Error>;
    fn material(&mut self, ledger: &MaterialLedgerId, kind: &str) -> Result<i64, Self::Error>;
    fn electricity(&mut self, id: &str) -> Result<i64, Self::Error>;
    fn market_quotes(
        &mut self,
        ledger: &MaterialLedgerId,
        consume: &[MaterialStack],
    ) -> Result<Vec<MaterialMarketQuote>, Self::Error>;
    fn bottleneck_tags(
        &mut self,
        profile: Option<&crate::RecipeProfileV1>,
        consume: &[MaterialStack],
    ) -> Result<Vec<String>, Self::Error>;
    fn scarcity_delay(
        &mut self,
        preferred: &MaterialLedgerId,
        actual: &MaterialLedgerId,
        consume: &[MaterialStack],
        tags: &[String],
    ) -> Result<u32, Self::Error>;
}
pub(in crate::world) struct LegacyFactoryRecipeFactsV1<'a> {
    world: &'a World,
}
impl<'a> LegacyFactoryRecipeFactsV1<'a> {
    pub(in crate::world) fn new(world: &'a World) -> Self {
        Self { world }
    }
}
impl sealed::Sealed for LegacyFactoryRecipeFactsV1<'_> {}
impl<'a> FactoryRecipeFactsV1<'a> for LegacyFactoryRecipeFactsV1<'a> {
    type Error = WorldError;
    fn charge(&mut self, _: usize) -> Result<(), Self::Error> {
        Ok(())
    }
    fn reserve_recipe_output(
        &mut self,
        _: &str,
        _: &str,
        _: &str,
        _: &oasis7_wasm_abi::RecipeExecutionPlan,
        _: &[String],
        _: &[String],
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    fn now(&self) -> u64 {
        self.world.state.time
    }
    fn agent_exists(&mut self, id: &str) -> Result<bool, Self::Error> {
        Ok(self.world.state.agents.contains_key(id))
    }
    fn factory(&mut self, id: &str) -> Result<Option<RecipeFactoryV1<'a>>, Self::Error> {
        Ok(self
            .world
            .state
            .factories
            .get(id)
            .map(RecipeFactoryV1::Legacy))
    }
    fn pending_recipes_for_factory(&mut self, id: &str) -> Result<usize, Self::Error> {
        Ok(self
            .world
            .state
            .pending_recipe_jobs
            .values()
            .filter(|job| job.factory_id == id)
            .count())
    }
    fn output_stack_rejection(
        &mut self,
        plan: &oasis7_wasm_abi::RecipeExecutionPlan,
    ) -> Result<Option<RejectReason>, Self::Error> {
        self.world.recipe_output_stack_limit_rejection(plan)
    }
    fn stage(&mut self) -> Result<crate::IndustryStage, Self::Error> {
        Ok(self.world.state.industry_progress.stage)
    }
    fn recipe_profile(
        &mut self,
        id: &str,
    ) -> Result<Option<FactoryScheduleRecordV1<'a, crate::RecipeProfileV1>>, Self::Error> {
        Ok(self
            .world
            .recipe_profile(id)
            .map(FactoryScheduleRecordV1::Legacy))
    }
    fn product_profile(
        &mut self,
        id: &str,
    ) -> Result<Option<FactoryScheduleRecordV1<'a, crate::ProductProfileV1>>, Self::Error> {
        Ok(self
            .world
            .product_profile(id)
            .map(FactoryScheduleRecordV1::Legacy))
    }
    fn effective_consume(
        &mut self,
        consume: &[MaterialStack],
        produce: &[MaterialStack],
    ) -> Result<Vec<MaterialStack>, Self::Error> {
        Ok(
            super::action_to_event_economy_support::merge_recipe_consume_with_maintenance_sink(
                self.world, consume, produce,
            ),
        )
    }
    fn route(
        &mut self,
        id: &str,
    ) -> Result<
        Option<FactoryScheduleRecordV1<'a, oasis7_world_state::LogisticsRouteV1>>,
        Self::Error,
    > {
        Ok(self
            .world
            .state
            .logistics_routes
            .get(id)
            .map(FactoryScheduleRecordV1::Legacy))
    }
    fn route_completed(&mut self, id: &str) -> Result<bool, Self::Error> {
        Ok(self.world.state.completed_logistics_route_ids.contains(id))
    }
    fn path_id(&mut self, routes: &[String]) -> Result<Option<String>, Self::Error> {
        Ok(logistics_path_id(routes))
    }
    fn allocate_paths(
        &mut self,
        ledger: &MaterialLedgerId,
        routes: &[String],
        paths: &[String],
        consume: &[MaterialStack],
    ) -> Result<Result<(), String>, Self::Error> {
        Ok(self
            .world
            .state
            .allocate_recipe_path_amounts(ledger, routes, paths, consume)
            .map(|_| ()))
    }
    fn material(&mut self, ledger: &MaterialLedgerId, kind: &str) -> Result<i64, Self::Error> {
        Ok(self.world.ledger_material_balance(ledger, kind))
    }
    fn electricity(&mut self, id: &str) -> Result<i64, Self::Error> {
        Ok(self
            .world
            .state
            .agents
            .get(id)
            .map(|cell| {
                cell.state
                    .resources
                    .get(oasis7_agent_api::world_action::ResourceKind::Electricity)
            })
            .unwrap_or(0))
    }
    fn market_quotes(
        &mut self,
        ledger: &MaterialLedgerId,
        consume: &[MaterialStack],
    ) -> Result<Vec<MaterialMarketQuote>, Self::Error> {
        Ok(
            super::action_to_event_economy_support::build_material_market_quotes(
                self.world, ledger, consume,
            ),
        )
    }
    fn bottleneck_tags(
        &mut self,
        profile: Option<&crate::RecipeProfileV1>,
        consume: &[MaterialStack],
    ) -> Result<Vec<String>, Self::Error> {
        Ok(
            super::action_to_event_economy_support::resolve_recipe_bottleneck_tags(
                profile, consume,
            ),
        )
    }
    fn scarcity_delay(
        &mut self,
        preferred: &MaterialLedgerId,
        actual: &MaterialLedgerId,
        consume: &[MaterialStack],
        tags: &[String],
    ) -> Result<u32, Self::Error> {
        Ok(
            super::action_to_event_economy_support::compute_local_scarcity_delay_ticks(
                self.world, preferred, actual, consume, tags,
            ),
        )
    }
}
