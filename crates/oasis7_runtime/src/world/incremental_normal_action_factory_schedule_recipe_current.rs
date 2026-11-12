//! Genuine Current observations for the original direct Recipe kernel.
use super::*;
use crate::world::event_processing::factory_schedule_fixed::{
    FactoryRecipeFactsV1, FactoryScheduleRecordV1, RecipeFactoryV1,
};
use crate::world::event_processing::{
    factory_schedule_market_fixed as market, factory_schedule_product_fixed as product,
};
use current::{CurrentFactoryScheduleFactsV1, EvaluationErrorV1};
use oasis7_world_state::{
    MaterialLedgerId, MaterialStack,
    incremental_factory_lifecycle_index::observe_current_factory_lifecycle_v1,
    incremental_factory_schedule_factory::recover_current_schedule_factory_v1,
    incremental_factory_schedule_logistics::{
        recover_current_schedule_path_v1, recover_current_schedule_route_v1,
    },
    incremental_world_id_sets::{
        WorldIdSetFieldV1, WorldIdSetKeyRefV1, world_id_set_contains_overlay_v1,
    },
    incremental_world_native::{
        recover_current_schedule_product_profile_v1, recover_current_schedule_recipe_profile_v1,
    },
};
use std::{
    borrow::{Borrow, Cow},
    cell::RefCell,
    collections::BTreeMap,
};

pub(super) struct PaidPath(ReservedValue<oasis7_world_state::LogisticsPathAuthorityV1>);
impl Borrow<oasis7_world_state::LogisticsPathAuthorityV1> for PaidPath {
    fn borrow(&self) -> &oasis7_world_state::LogisticsPathAuthorityV1 {
        &self.0
    }
}
pub(super) struct RecipePathsPlanV1 {
    pub paths: BTreeMap<String, PaidPath>,
    pub allocations: BTreeMap<String, i64>,
}
impl RecipePathsPlanV1 {
    pub(super) fn path(&self, id: &str) -> Option<&oasis7_world_state::LogisticsPathAuthorityV1> {
        self.paths.get(id).map(Borrow::borrow)
    }
}

impl<M, P, R, V, E, C, PE, PC>
    CurrentFactoryScheduleFactsV1<'_, '_, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
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
    pub(super) fn path_plan(
        &mut self,
        ledger: &MaterialLedgerId,
        routes: &[String],
        paths: &[String],
        consume: &[MaterialStack],
    ) -> Result<Result<RecipePathsPlanV1, String>, EvaluationErrorV1> {
        self.charge(0)?;
        if paths.is_empty() {
            return Ok(Ok(RecipePathsPlanV1 {
                paths: BTreeMap::new(),
                allocations: BTreeMap::new(),
            }));
        }
        let sources = self.sources;
        let live = self.live;
        let mut selected = BTreeMap::new();
        let mut route_count = 0usize;
        let mut payload = 0usize;
        for id in paths {
            self.charge(id.len())?;
            if selected.contains_key(id) {
                continue;
            }
            let record = super::io::paid(
                sources.roots,
                self.context,
                self.read,
                self.work,
                self.common,
                |ctx, _read, work| {
                    let guard = ctx.transaction.guard();
                    Ok(recover_current_schedule_path_v1(
                        id,
                        sources.logistics,
                        ctx,
                        &mut work.logistics,
                        &guard,
                        live,
                    )?)
                },
            )?;
            // True absence stays absent in the selected data map. The unique
            // allocator decides its business error at the original sorted-path
            // position, after requirement aggregation and duplicate checks.
            let Some(record) = record else {
                continue;
            };
            // Actual paths remain wrapped and paid. Only the original integer
            // quantity and authentic original strings reach the allocator.
            route_count = route_count
                .checked_add(record.route_ids.len())
                .ok_or_else(limited)?;
            for text in std::iter::once(&record.kind).chain(record.route_ids.iter()) {
                self.charge(text.len())?;
                payload = payload.checked_add(text.len()).ok_or_else(limited)?;
            }
            let n = super::bounds::node::<String, PaidPath>()
                .and_then(|n| n.checked_add(id.len()))
                .ok_or_else(limited)?;
            self.fees.hold(n, self.common)?;
            selected.insert(id.clone(), PaidPath(record));
        }
        // Original allocation simultaneously owns required-kind/paths-by-kind/
        // result maps, seen/submitted/path-route sets and sorted path strings.
        // Each potential input is admitted as a full internal node; nested Vec
        // growth is <=4 per group +2 per path, hence <=6*paths total slots.
        let mut heap = 0usize;
        for (count, node) in [
            (consume.len(), super::bounds::node::<String, i64>()),
            (
                paths.len(),
                super::bounds::node::<String, Vec<(String, i64)>>(),
            ),
            (paths.len(), super::bounds::node::<String, i64>()),
            (paths.len(), super::bounds::node::<String, ()>()),
            (routes.len(), super::bounds::node::<String, ()>()),
            (route_count, super::bounds::node::<String, ()>()),
        ] {
            heap = heap
                .checked_add(
                    count
                        .checked_mul(node.ok_or_else(limited)?)
                        .ok_or_else(limited)?,
                )
                .ok_or_else(limited)?;
        }
        heap = heap
            .checked_add(
                paths
                    .len()
                    .checked_mul(6 * size_of::<(String, i64)>() + size_of::<String>())
                    .ok_or_else(limited)?,
            )
            .ok_or_else(limited)?;
        for text in paths.iter().chain(routes) {
            self.charge(text.len())?;
            payload = payload.checked_add(text.len()).ok_or_else(limited)?;
        }
        for stack in consume {
            self.charge(stack.kind.len())?;
            payload = payload.checked_add(stack.kind.len()).ok_or_else(limited)?;
        }
        // Two possibly coexisting growable error/note strings, each with the
        // worst Debug escape expansion. All allocator templates together are
        // shorter than this explicitly listed combined template inventory.
        const LABELS: &str = "recipe path authority requirement overflow: kind= amount=duplicate logistics path binding: path_id=logistics path is not completed: path_id=logistics path destination mismatch: path_id=logistics path material mismatch: path_id=logistics path quantity authority is invalid: path_id=logistics path recipe authority is exhausted: path_id=logistics route binding does not match completed path authoritylogistics path quantity overflow: kind=logistics path quantity insufficient: kind= requested= available=factory schedule path work";
        heap = heap
            .checked_add(payload.checked_mul(24).ok_or_else(limited)?)
            .and_then(|n| n.checked_add(4 * (LABELS.len() + 3 * 20)))
            .ok_or_else(limited)?;
        self.fees.hold(heap, self.common)?;
        let mut first = None;
        let result = oasis7_world_state::allocate_recipe_path_amounts_from_records_v1(
            &selected,
            ledger,
            routes,
            paths,
            consume,
            &mut |n| match self.charge(n) {
                Ok(()) => Ok(()),
                Err(error) => {
                    if first.is_none() {
                        first = Some(error);
                    }
                    Err("factory schedule path work".to_owned())
                }
            },
        );
        if let Some(error) = first {
            return Err(error);
        }
        Ok(result.map(|allocations| RecipePathsPlanV1 {
            paths: selected,
            allocations,
        }))
    }
}

impl<M, P, R, V, E, C, PE, PC> product::sealed::Sealed
    for CurrentFactoryScheduleFactsV1<'_, '_, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
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
}
impl<M, P, R, V, E, C, PE, PC> product::ProductStackLimitFactsV1
    for CurrentFactoryScheduleFactsV1<'_, '_, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
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
    type Error = EvaluationErrorV1;
    fn charge(&mut self, n: usize) -> Result<(), Self::Error> {
        self.charge(n)
    }
    fn reserve_result(&mut self, kind: &str) -> Result<(), Self::Error> {
        let n=kind.len().checked_mul(24).and_then(|n|n.checked_add(4*("recipe produce byproduct stack limit exceeded: kind= amount= limit=; reduce batches and request a new quote".len()+2*20))).ok_or_else(limited)?;
        self.fees.hold(n, self.common)?;
        Ok(())
    }
    fn resolve_product_module(&mut self, kind: &str) -> Result<Option<String>, Self::Error> {
        self.product_module_id(kind)
    }
    fn active_module_hash(&mut self, id: &str) -> Result<Cow<'_, str>, Self::Error> {
        Ok(Cow::Owned(self.product_module_hash(id)?))
    }
}
impl<M, P, R, V, E, C, PE, PC> market::sealed::Sealed
    for CurrentFactoryScheduleFactsV1<'_, '_, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
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
}
impl<M, P, R, V, E, C, PE, PC> market::FactoryMarketFactsV1
    for CurrentFactoryScheduleFactsV1<'_, '_, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
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
    type Error = EvaluationErrorV1;
    fn charge(&mut self, n: usize) -> Result<(), Self::Error> {
        self.charge(n)
    }
    fn reserve_work(
        &mut self,
        consume: &[MaterialStack],
        tags: &[String],
    ) -> Result<(), Self::Error> {
        let mut n = consume
            .len()
            .checked_mul(
                super::bounds::node::<String, i64>()
                    .ok_or_else(limited)?
                    .checked_add(size_of::<crate::MaterialMarketQuote>())
                    .ok_or_else(limited)?,
            )
            .ok_or_else(limited)?;
        for stack in consume {
            self.charge(stack.kind.len())?;
            n = n
                .checked_add(stack.kind.len().checked_mul(2).ok_or_else(limited)?)
                .ok_or_else(limited)?;
        }
        n = n
            .checked_add(super::bounds::tag_normalization_bytes(tags)?)
            .ok_or_else(limited)?;
        self.fees.hold(n, self.common)?;
        Ok(())
    }
    fn material(&mut self, ledger: &MaterialLedgerId, kind: &str) -> Result<i64, Self::Error> {
        self.quantity(ledger, kind)
    }
    fn transit_loss_bps(&mut self, kind: &str) -> Result<i64, Self::Error> {
        self.charge(kind.len())?;
        let sources = self.sources;
        let live = self.live;
        let class = super::io::paid(
            sources.roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, read, work| {
                let guard = ctx.transaction.guard();
                Ok(oasis7_world_state::incremental_material_records::read_current_material_transport_loss_overlay_v1(kind,sources.material_profiles,read.state,ctx,&mut work.material_profiles,&guard,live)?)
            },
        )?;
        let factor = match class {
            None | Some(oasis7_wasm_abi::MaterialTransportLossClass::Low) => 1,
            Some(oasis7_wasm_abi::MaterialTransportLossClass::Medium) => 2,
            Some(oasis7_wasm_abi::MaterialTransportLossClass::High) => 4,
        };
        Ok(
            oasis7_world_state::world::pure_helpers::MATERIAL_TRANSFER_LOSS_PER_KM_BPS
                .max(0)
                .saturating_mul(factor),
        )
    }
    fn governance_tax_bps(&mut self) -> Result<u16, Self::Error> {
        let sources = self.sources;
        let live = self.live;
        let policy = super::io::paid(
            sources.roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, parent, work| {
                let mut read = WorldFieldReadContextV1 {
                    root: sources.roots.state_root(
                        oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
                    ),
                    state: parent.state,
                    values: ValueReadContext {
                        source: parent.values.source,
                        memory: parent.values.memory,
                        class: parent.values.class,
                        io: &mut *parent.values.io,
                    },
                    key_work: &mut *parent.key_work,
                    semantic_bytes: parent.semantic_bytes,
                    record_bytes: parent.record_bytes,
                    take_state_error: &mut parent.take_state_error,
                    cancelled: &mut parent.cancelled,
                };
                Ok(oasis7_world_state::incremental_world_native::recover_gameplay_policy_overlay_v1(ctx.transaction,live,&mut read,&mut work.lifecycle.native_scan,&mut work.lifecycle.native_read)?)
            },
        )?;
        let p = policy.value().value();
        self.charge(4)?;
        Ok(p.electricity_tax_bps
            .saturating_add(p.data_tax_bps)
            .min(10_000))
    }
}

impl<'source, M, P, R, V, E, C, PE, PC> FactoryRecipeFactsV1<'source>
    for CurrentFactoryScheduleFactsV1<'_, 'source, '_, '_, '_, '_, '_, M, P, R, V, E, C, PE, PC>
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
    type Error = EvaluationErrorV1;
    fn charge(&mut self, n: usize) -> Result<(), Self::Error> {
        self.charge(n)
    }
    fn reserve_recipe_output(
        &mut self,
        requester: &str,
        factory: &str,
        recipe: &str,
        plan: &oasis7_wasm_abi::RecipeExecutionPlan,
        routes: &[String],
        paths: &[String],
    ) -> Result<(), Self::Error> {
        let mut n = 0usize;
        for text in [requester, factory, recipe]
            .into_iter()
            .chain(routes.iter().map(String::as_str))
            .chain(paths.iter().map(String::as_str))
        {
            self.charge(text.len())?;
            n = n.checked_add(text.len()).ok_or_else(limited)?;
        }
        for stack in plan
            .consume
            .iter()
            .chain(&plan.produce)
            .chain(&plan.byproducts)
        {
            self.charge(stack.kind.len())?;
            n = n.checked_add(stack.kind.len()).ok_or_else(limited)?;
        }
        if let Some(reason) = &plan.reject_reason {
            self.charge(reason.len())?;
            n = n.checked_add(reason.len()).ok_or_else(limited)?;
        }
        let producer_slots = plan
            .produce
            .len()
            .checked_add(plan.byproducts.len())
            .and_then(|n| n.checked_mul(size_of::<MaterialStack>()))
            .ok_or_else(limited)?;
        let route_slots = routes
            .len()
            .checked_mul(3 * size_of::<String>())
            .and_then(|n| n.checked_add(paths.len().checked_mul(size_of::<String>())?))
            .ok_or_else(limited)?;
        let route_nodes = routes
            .len()
            .checked_mul(
                super::bounds::node::<&String, ()>()
                    .and_then(|n| n.checked_add(super::bounds::node::<String, ()>()?))
                    .ok_or_else(limited)?,
            )
            .ok_or_else(limited)?;
        let n = n
            .checked_mul(24)
            .and_then(|n| n.checked_add(producer_slots))
            .and_then(|n| n.checked_add(route_slots))
            .and_then(|n| n.checked_add(route_nodes))
            .and_then(|n| n.checked_add(super::bounds::recipe_output_bytes()?))
            .and_then(|n| {
                n.checked_add(
                    routes
                        .len()
                        .checked_add(paths.len())?
                        .checked_mul(size_of::<String>())?,
                )
            })
            .ok_or_else(limited)?;
        self.fees.hold(n, self.common)?;
        Ok(())
    }
    fn now(&self) -> u64 {
        self.now
    }
    fn agent_exists(&mut self, id: &str) -> Result<bool, Self::Error> {
        self.presence(id)
    }
    fn factory(&mut self, id: &str) -> Result<Option<RecipeFactoryV1<'source>>, Self::Error> {
        self.charge(id.len())?;
        let sources = self.sources;
        let live = self.live;
        let record = super::io::paid(
            sources.roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, _read, work| {
                let guard = ctx.transaction.guard();
                Ok(recover_current_schedule_factory_v1(
                    id,
                    sources.lifecycle.factories,
                    ctx,
                    &mut work.factory,
                    &guard,
                    live,
                )?)
            },
        )?;
        if let Some(record) = &record {
            let mut n = record.builder().len();
            for text in record.tags() {
                self.charge(text.len())?;
                n = n.checked_add(text.len()).ok_or_else(limited)?;
            }
            for ledger in [record.input(), record.output()] {
                match ledger {
                    MaterialLedgerId::World => {}
                    MaterialLedgerId::Agent(v)
                    | MaterialLedgerId::Site(v)
                    | MaterialLedgerId::Factory(v) => {
                        self.charge(v.len())?;
                        n = n.checked_add(v.len()).ok_or_else(limited)?;
                    }
                }
            }
            self.fees.hold(
                n.checked_mul(24)
                    .and_then(|n| {
                        n.checked_add(super::bounds::tag_normalization_bytes(record.tags()).ok()?)
                    })
                    .ok_or_else(limited)?,
                self.common,
            )?;
        }
        Ok(record.map(RecipeFactoryV1::Current))
    }
    fn pending_recipes_for_factory(&mut self, id: &str) -> Result<usize, Self::Error> {
        self.charge(id.len())?;
        let sources = self.sources;
        let live = self.live;
        let value = super::io::paid(
            sources.roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, read, work| {
                let guard = ctx.transaction.guard();
                Ok(observe_current_factory_lifecycle_v1(
                    id,
                    sources.lifecycle.index,
                    sources.lifecycle.factories,
                    sources.lifecycle.industry,
                    sources.roots,
                    ctx.transaction,
                    read,
                    &mut work.lifecycle.factory,
                    &mut work.lifecycle.industry,
                    &mut work.lifecycle.lifecycle,
                    work.factory.max_text_bytes as u64,
                    &guard,
                    live,
                )?)
            },
        )?;
        value.validate(self.context.transaction, self.common.memory(), self.live)?;
        Ok(value.active_jobs())
    }
    fn output_stack_rejection(
        &mut self,
        plan: &oasis7_wasm_abi::RecipeExecutionPlan,
    ) -> Result<Option<crate::RejectReason>, Self::Error> {
        product::recipe_output_stack_limit_rejection_v1(self, plan)
    }
    fn stage(&mut self) -> Result<crate::IndustryStage, Self::Error> {
        let progress = self.progress()?;
        Ok(progress.value().ok_or_else(invalid)?.stage)
    }
    fn recipe_profile(
        &mut self,
        id: &str,
    ) -> Result<Option<FactoryScheduleRecordV1<'source, crate::RecipeProfileV1>>, Self::Error> {
        self.charge(id.len())?;
        let sources = self.sources;
        let live = self.live;
        let record = super::io::paid(
            sources.roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, parent, work| {
                let guard = ctx.transaction.guard();
                let mut read = WorldFieldReadContextV1 {
                    root: sources.roots.state_root(
                        oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
                    ),
                    state: parent.state,
                    values: ValueReadContext {
                        source: parent.values.source,
                        memory: parent.values.memory,
                        class: parent.values.class,
                        io: &mut *parent.values.io,
                    },
                    key_work: &mut *parent.key_work,
                    semantic_bytes: parent.semantic_bytes,
                    record_bytes: parent.record_bytes,
                    take_state_error: &mut parent.take_state_error,
                    cancelled: &mut parent.cancelled,
                };
                Ok(recover_current_schedule_recipe_profile_v1(
                    id,
                    sources.lifecycle.native,
                    sources.roots,
                    ctx.transaction,
                    &mut read,
                    &mut work.lifecycle.native_scan,
                    &mut work.lifecycle.native_read,
                    &guard,
                    live,
                )?)
            },
        )?;
        if let Some(record) = &record {
            let value = record.value().value();
            self.diagnostics(value)?;
            self.fees.hold(
                super::bounds::tag_normalization_bytes(&value.preferred_factory_tags)?
                    .checked_add(super::bounds::tag_normalization_bytes(
                        &value.bottleneck_tags,
                    )?)
                    .ok_or_else(limited)?,
                self.common,
            )?;
        }
        Ok(record.map(FactoryScheduleRecordV1::Native))
    }
    fn product_profile(
        &mut self,
        id: &str,
    ) -> Result<Option<FactoryScheduleRecordV1<'source, crate::ProductProfileV1>>, Self::Error>
    {
        self.charge(id.len())?;
        let sources = self.sources;
        let live = self.live;
        let record = super::io::paid(
            sources.roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, parent, work| {
                let guard = ctx.transaction.guard();
                let mut read = WorldFieldReadContextV1 {
                    root: sources.roots.state_root(
                        oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
                    ),
                    state: parent.state,
                    values: ValueReadContext {
                        source: parent.values.source,
                        memory: parent.values.memory,
                        class: parent.values.class,
                        io: &mut *parent.values.io,
                    },
                    key_work: &mut *parent.key_work,
                    semantic_bytes: parent.semantic_bytes,
                    record_bytes: parent.record_bytes,
                    take_state_error: &mut parent.take_state_error,
                    cancelled: &mut parent.cancelled,
                };
                Ok(recover_current_schedule_product_profile_v1(
                    id,
                    sources.lifecycle.native,
                    sources.roots,
                    ctx.transaction,
                    &mut read,
                    &mut work.lifecycle.native_scan,
                    &mut work.lifecycle.native_read,
                    &guard,
                    live,
                )?)
            },
        )?;
        if let Some(record) = &record {
            self.diagnostics(record.value().value())?;
        }
        Ok(record.map(FactoryScheduleRecordV1::Native))
    }
    fn effective_consume(
        &mut self,
        consume: &[MaterialStack],
        produce: &[MaterialStack],
    ) -> Result<Vec<MaterialStack>, Self::Error> {
        let source = RefCell::new(self);
        oasis7_world_state::state::apply_domain_event_industry_helpers::recipe_consume_with_maintenance_from_profiles_v1(consume,produce,&mut |kind|FactoryRecipeFactsV1::product_profile(&mut **source.borrow_mut(),kind),&mut |n|{let mut source=source.borrow_mut();let source=&mut **source;source.fees.hold(n,source.common).map_err(Into::into)})
    }
    fn route(
        &mut self,
        id: &str,
    ) -> Result<
        Option<FactoryScheduleRecordV1<'source, oasis7_world_state::LogisticsRouteV1>>,
        Self::Error,
    > {
        self.charge(id.len())?;
        let sources = self.sources;
        let live = self.live;
        let value = super::io::paid(
            sources.roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, _read, work| {
                let guard = ctx.transaction.guard();
                Ok(recover_current_schedule_route_v1(
                    id,
                    sources.logistics,
                    ctx,
                    &mut work.logistics,
                    &guard,
                    live,
                )?)
            },
        )?;
        if let Some(value) = &value {
            self.diagnostics(&**value)?;
        }
        Ok(value.map(FactoryScheduleRecordV1::Current))
    }
    fn route_completed(&mut self, id: &str) -> Result<bool, Self::Error> {
        self.charge(id.len())?;
        let roots = self.sources.roots;
        super::io::paid(
            roots,
            self.context,
            self.read,
            self.work,
            self.common,
            |ctx, _read, _work| {
                Ok(world_id_set_contains_overlay_v1(
                    WorldIdSetFieldV1::CompletedLogisticsRoutes,
                    WorldIdSetKeyRefV1::Text(id),
                    ctx,
                )?)
            },
        )
    }
    fn path_id(&mut self, routes: &[String]) -> Result<Option<String>, Self::Error> {
        if routes.is_empty() {
            return Ok(None);
        }
        let hash = crate::world::incremental_normal_action_diplomacy::fixed_json_digest_v1(
            routes,
            self.common,
        )?;
        self.fees.hold(hash.len(), self.common)?;
        Ok(Some((*hash).clone()))
    }
    fn allocate_paths(
        &mut self,
        ledger: &MaterialLedgerId,
        routes: &[String],
        paths: &[String],
        consume: &[MaterialStack],
    ) -> Result<Result<(), String>, Self::Error> {
        Ok(self.path_plan(ledger, routes, paths, consume)?.map(|_| ()))
    }
    fn material(&mut self, ledger: &MaterialLedgerId, kind: &str) -> Result<i64, Self::Error> {
        self.quantity(ledger, kind)
    }
    fn electricity(&mut self, id: &str) -> Result<i64, Self::Error> {
        self.electricity_value(id)
    }
    fn market_quotes(
        &mut self,
        ledger: &MaterialLedgerId,
        consume: &[MaterialStack],
    ) -> Result<Vec<crate::MaterialMarketQuote>, Self::Error> {
        market::market_quotes_from_facts_v1(self, ledger, consume)
    }
    fn bottleneck_tags(
        &mut self,
        profile: Option<&crate::RecipeProfileV1>,
        consume: &[MaterialStack],
    ) -> Result<Vec<String>, Self::Error> {
        let mut n = 0usize;
        for stack in consume {
            self.charge(stack.kind.len())?;
            n = n.checked_add(stack.kind.len()).ok_or_else(limited)?;
        }
        let entries = profile.map_or(consume.len(), |p| {
            p.bottleneck_tags
                .len()
                .checked_add(consume.len())
                .unwrap_or(usize::MAX)
        });
        let heap =
            oasis7_world_state::incremental_collection_layout::string_set_output_heap_v1(entries)?
                .checked_add(
                    entries
                        .checked_mul(size_of::<String>())
                        .ok_or_else(limited)?,
                )
                .and_then(|v| v.checked_add(n))
                .ok_or_else(limited)?;
        self.fees.hold(heap, self.common)?;
        Ok(
            crate::world::event_processing::factory_schedule_fixed::resolve_recipe_bottleneck_tags(
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
        market::scarcity_delay_from_facts_v1(self, preferred, actual, consume, tags)
    }
}
