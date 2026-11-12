//! Fixed due-order driver for the original material-transit normal-tail phase.
//!
//! Parent identities select only candidate ids. Every due candidate is read
//! again from transaction Current, reduced, published through the private
//! counted system-event owner, and routed before the next candidate begins.

use super::*;
use crate::world::{
    incremental_body::BodyMutationError,
    incremental_body_event::materials::CorePolicyMaterialContextV1,
    incremental_module_invocation::{
        AcceptedInvocationDirectoryV1, CurrentInvocationEnumerationWorkV1,
    },
    incremental_normal_sources::BoundNormalTickParentV1,
    incremental_normal_tail::{NormalTailSourcesV1, NormalTailWorkV1},
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::ChildLeaseV1,
    incremental_execution::{ExecutionError, ExecutionFence},
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::{
    incremental_field_read::WorldFieldReadContextV1,
    incremental_industry_jobs::{IndustryFamilyReadContextV1, IndustryWorkBudgetV1},
    incremental_logistics_records::LogisticsWorkBudgetV1,
};

#[derive(Default)]
struct LaneTotalsV1 {
    steps: u128,
    bytes: u128,
}

impl LaneTotalsV1 {
    fn step(&mut self, value: &mut u64, ceiling: Option<u64>) {
        if let Some(ceiling) = ceiling {
            *value = (*value).min(ceiling);
        }
        self.steps += u128::from(*value);
    }

    fn entries(
        &mut self,
        value: &mut usize,
        ceiling: Option<u64>,
    ) -> Result<(), BodyMutationError> {
        if let Some(ceiling) = ceiling {
            let ceiling = usize::try_from(ceiling).unwrap_or(usize::MAX);
            *value = (*value).min(ceiling);
        }
        self.steps += u128::try_from(*value)
            .map_err(|_| ValueTreeError::ResourceLimited("normal transit work lane"))?;
        Ok(())
    }

    fn byte(&mut self, value: &mut u64, ceiling: Option<u64>) {
        if let Some(ceiling) = ceiling {
            *value = (*value).min(ceiling);
        }
        self.bytes += u128::from(*value);
    }
}

fn scan_lanes(
    total: &mut LaneTotalsV1,
    scan: &mut oasis7_formal_protocol::incremental_state::StateScanBudget,
    step_ceiling: Option<u64>,
    byte_ceiling: Option<u64>,
) {
    total.step(&mut scan.nodes_remaining, step_ceiling);
    total.byte(&mut scan.read_bytes_remaining, byte_ceiling);
    total.step(&mut scan.leaves_remaining, step_ceiling);
}

fn io_lanes(
    total: &mut LaneTotalsV1,
    io: &mut oasis7_formal_protocol::incremental_value::ValueIoBudget,
    step_ceiling: Option<u64>,
    byte_ceiling: Option<u64>,
) {
    total.step(&mut io.nodes_remaining, step_ceiling);
    total.byte(&mut io.read_bytes_remaining, byte_ceiling);
    total.byte(&mut io.write_bytes_remaining, byte_ceiling);
}

fn lane_delta(
    before: LaneTotalsV1,
    after: LaneTotalsV1,
    lease: &ChildLeaseV1<'_>,
) -> Result<(), BodyMutationError> {
    let Some(steps) = before
        .steps
        .checked_sub(after.steps)
        .and_then(|n| u64::try_from(n).ok())
    else {
        let _ = lease.consume(u64::MAX, u64::MAX);
        return Err(ValueTreeError::ResourceLimited("normal transit step accounting").into());
    };
    let Some(bytes) = before
        .bytes
        .checked_sub(after.bytes)
        .and_then(|n| u64::try_from(n).ok())
    else {
        let _ = lease.consume(u64::MAX, u64::MAX);
        return Err(ValueTreeError::ResourceLimited("normal transit byte accounting").into());
    };
    lease
        .consume(steps, bytes)
        .map_err(|error| BodyMutationError::Original(Box::new(error)))
}

fn selection_lanes<IM, IR, IE, IC, P, V, CE, CC, MW, MC>(
    industry_work: &mut IndustryWorkBudgetV1,
    industry_read: &mut IndustryFamilyReadContextV1<'_, IM, IR, IE, IC>,
    current_read: &mut WorldFieldReadContextV1<'_, P, V, CE, CC>,
    mutation: &mut crate::world::incremental_body::BodyMutationContext<'_, MW, MC>,
    ceiling: Option<(u64, u64)>,
) -> Result<LaneTotalsV1, BodyMutationError>
where
    IM: MerkleNodeReader,
    IR: ValueNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    IE: FnMut() -> Option<ValueTreeError>,
    IC: FnMut() -> bool,
    CE: FnMut() -> Option<ValueTreeError>,
    CC: FnMut() -> bool,
    MW: ValueNodeWriter,
    MC: FnMut() -> bool,
{
    let (step_ceiling, byte_ceiling) = ceiling.map_or((None, None), |(s, b)| (Some(s), Some(b)));
    let mut total = LaneTotalsV1::default();
    total.step(&mut industry_work.cells_remaining, step_ceiling);
    scan_lanes(
        &mut total,
        &mut industry_work.scan,
        step_ceiling,
        byte_ceiling,
    );
    scan_lanes(
        &mut total,
        industry_read.outer_scan,
        step_ceiling,
        byte_ceiling,
    );
    io_lanes(
        &mut total,
        &mut industry_read.values.io,
        step_ceiling,
        byte_ceiling,
    );
    total.byte(&mut industry_read.key_work.bytes_remaining, byte_ceiling);
    io_lanes(
        &mut total,
        &mut current_read.values.io,
        step_ceiling,
        byte_ceiling,
    );
    total.byte(&mut current_read.key_work.bytes_remaining, byte_ceiling);
    io_lanes(&mut total, &mut mutation.io, step_ceiling, byte_ceiling);
    Ok(total)
}

fn reducer_lanes<P, R, CE, CC, AE, AC, ME, W, MM, MC>(
    work: &mut crate::world::incremental_action_factory_lifecycle::FactoryLifecycleWorkV1,
    logistics: &mut LogisticsWorkBudgetV1,
    current_read: &mut WorldFieldReadContextV1<'_, P, R, CE, CC>,
    agent_read: &mut WorldFieldReadContextV1<'_, P, R, AE, AC>,
    materials: &mut CorePolicyMaterialContextV1<'_, MM, ME>,
    mutation: &mut crate::world::incremental_body::BodyMutationContext<'_, W, MC>,
    ceiling: Option<(u64, u64)>,
) -> Result<LaneTotalsV1, BodyMutationError>
where
    P: MerkleNodeReader,
    R: ValueNodeReader,
    CE: FnMut() -> Option<ValueTreeError>,
    CC: FnMut() -> bool,
    AE: FnMut() -> Option<ValueTreeError>,
    AC: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    MM: MerkleNodeReader,
    W: ValueNodeWriter,
    MC: FnMut() -> bool,
{
    let (step_ceiling, byte_ceiling) = ceiling.map_or((None, None), |(s, b)| (Some(s), Some(b)));
    let mut total = LaneTotalsV1::default();
    total.step(&mut work.steps_remaining, step_ceiling);
    total.byte(&mut work.bytes_remaining, byte_ceiling);
    total.entries(&mut work.pending_entries_remaining, step_ceiling)?;
    total.step(&mut work.records_remaining, step_ceiling);

    total.step(&mut work.factory.cells_remaining, step_ceiling);
    scan_lanes(
        &mut total,
        &mut work.factory.scan,
        step_ceiling,
        byte_ceiling,
    );
    total.step(&mut work.industry.cells_remaining, step_ceiling);
    scan_lanes(
        &mut total,
        &mut work.industry.scan,
        step_ceiling,
        byte_ceiling,
    );
    total.step(&mut work.lifecycle.cells_remaining, step_ceiling);
    total.byte(&mut work.lifecycle.comparison_bytes_remaining, byte_ceiling);
    total.entries(&mut work.lifecycle.pending_entries_remaining, step_ceiling)?;
    scan_lanes(
        &mut total,
        &mut work.native_scan,
        step_ceiling,
        byte_ceiling,
    );
    total.step(&mut work.native_read.json.cells_remaining, step_ceiling);
    total.step(
        &mut work.native_read.json.json_nodes_remaining,
        step_ceiling,
    );
    total.step(&mut work.native_write.cells_remaining, step_ceiling);
    total.step(&mut work.native_write.json.cells_remaining, step_ceiling);
    total.step(
        &mut work.native_write.json.json_nodes_remaining,
        step_ceiling,
    );
    total.entries(
        &mut work.output_group.pending_entries_remaining,
        step_ceiling,
    )?;

    total.step(&mut logistics.cells_remaining, step_ceiling);
    scan_lanes(&mut total, &mut logistics.scan, step_ceiling, byte_ceiling);

    io_lanes(
        &mut total,
        &mut current_read.values.io,
        step_ceiling,
        byte_ceiling,
    );
    total.byte(&mut current_read.key_work.bytes_remaining, byte_ceiling);
    io_lanes(
        &mut total,
        &mut agent_read.values.io,
        step_ceiling,
        byte_ceiling,
    );
    total.byte(&mut agent_read.key_work.bytes_remaining, byte_ceiling);

    io_lanes(&mut total, materials.read_io, step_ceiling, byte_ceiling);
    total.byte(&mut materials.read_keys.bytes_remaining, byte_ceiling);
    total.byte(&mut materials.write_keys.bytes_remaining, byte_ceiling);
    total.step(&mut materials.family_work.records_remaining, step_ceiling);
    total.step(
        &mut materials.family_work.index_steps_remaining,
        step_ceiling,
    );
    total.entries(
        &mut materials.scope_work.pending_entries_remaining,
        step_ceiling,
    )?;
    total.byte(
        &mut materials.scope_work.comparison_bytes_remaining,
        byte_ceiling,
    );
    total.step(&mut materials.group_work.steps_remaining, step_ceiling);
    total.step(materials.normalization_cells_remaining, step_ceiling);
    total.byte(
        materials.normalization_comparison_bytes_remaining,
        byte_ceiling,
    );
    io_lanes(&mut total, &mut mutation.io, step_ceiling, byte_ceiling);
    Ok(total)
}

fn route_lanes<M, R, E, C, W, F, O>(
    root: &mut RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>,
    enumeration: &mut CurrentInvocationEnumerationWorkV1,
    ceiling: Option<(u64, u64)>,
) -> Result<LaneTotalsV1, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: oasis7_formal_protocol::incremental_material::OriginalObjectStreamReader,
{
    let (step_ceiling, byte_ceiling) = ceiling.map_or((None, None), |(s, b)| (Some(s), Some(b)));
    let mut total = LaneTotalsV1::default();
    let work = &mut *root.work;
    total.step(&mut work.input.visits_remaining, step_ceiling);
    total.byte(&mut work.input.bytes_remaining, byte_ceiling);
    total.step(&mut work.routing.nodes_remaining, step_ceiling);
    total.step(&mut work.routing.edges_remaining, step_ceiling);
    total.byte(&mut work.routing.string_bytes_remaining, byte_ceiling);
    total.step(&mut work.routing.match_steps_remaining, step_ceiling);
    total.step(&mut work.lookup_steps_remaining, step_ceiling);
    scan_lanes(
        &mut total,
        &mut work.anchor_scan,
        step_ceiling,
        byte_ceiling,
    );
    total.step(&mut work.anchor_cells_remaining, step_ceiling);
    total.entries(&mut work.visual.pending_entries_remaining, step_ceiling)?;
    scan_lanes(
        &mut total,
        &mut work.visual.body_scan,
        step_ceiling,
        byte_ceiling,
    );
    total.step(&mut work.visual.cells_remaining, step_ceiling);
    total.step(&mut work.visual_decode.nodes_remaining, step_ceiling);
    total.byte(&mut work.visual_decode.text_bytes_remaining, byte_ceiling);
    total.step(&mut work.effects.clone_nodes_remaining, step_ceiling);
    total.byte(&mut work.effects.clone_bytes_remaining, byte_ceiling);
    total.step(&mut work.effect_records.cells_remaining, step_ceiling);
    total.step(&mut work.effect_records.json_nodes_remaining, step_ceiling);
    total.step(&mut work.authorization.cells_remaining, step_ceiling);
    total.byte(
        &mut work.authorization.comparison_bytes_remaining,
        byte_ceiling,
    );
    total.step(&mut work.output.calls_remaining, step_ceiling);
    total.byte(&mut work.output.structure.bytes_remaining, byte_ceiling);
    total.step(&mut work.output.structure.values_remaining, step_ceiling);
    scan_lanes(
        &mut total,
        &mut work.selection_scan,
        step_ceiling,
        byte_ceiling,
    );
    total.step(&mut work.selection.cells_remaining, step_ceiling);
    total.step(&mut work.selection.json.cells_remaining, step_ceiling);
    total.step(&mut work.selection.json.json_nodes_remaining, step_ceiling);
    total.byte(
        &mut work.selection_limits.manifest_json_bytes_remaining,
        byte_ceiling,
    );
    total.step(&mut work.original.objects_remaining, step_ceiling);
    total.byte(&mut work.original.bytes_remaining, byte_ceiling);

    scan_lanes(
        &mut total,
        &mut root.manifest_work.body_scan,
        step_ceiling,
        byte_ceiling,
    );
    total.step(&mut root.manifest_work.fields.cells_remaining, step_ceiling);
    total.step(
        &mut root.manifest_work.fields.json.cells_remaining,
        step_ceiling,
    );
    total.step(
        &mut root.manifest_work.fields.json.json_nodes_remaining,
        step_ceiling,
    );
    total.step(&mut root.manifest_work.json.cells_remaining, step_ceiling);
    total.step(
        &mut root.manifest_work.json.json_nodes_remaining,
        step_ceiling,
    );
    total.byte(&mut root.manifest_work.hash_bytes_remaining, byte_ceiling);

    scan_lanes(
        &mut total,
        &mut enumeration.instance_scan,
        step_ceiling,
        byte_ceiling,
    );
    scan_lanes(
        &mut total,
        &mut enumeration.registry_scan,
        step_ceiling,
        byte_ceiling,
    );
    total.entries(&mut enumeration.pending_entries, step_ceiling)?;
    total.byte(&mut enumeration.comparison_bytes_remaining, byte_ceiling);

    io_lanes(
        &mut total,
        &mut root.ctx.values.io,
        step_ceiling,
        byte_ceiling,
    );
    total.byte(&mut root.ctx.key_work.bytes_remaining, byte_ceiling);
    io_lanes(&mut total, root.members.io, step_ceiling, byte_ceiling);
    total.byte(&mut root.members.key_work.bytes_remaining, byte_ceiling);
    io_lanes(&mut total, root.fees.values.io, step_ceiling, byte_ceiling);
    total.byte(&mut root.fees.key_work.bytes_remaining, byte_ceiling);
    io_lanes(
        &mut total,
        &mut root.events.mutation.io,
        step_ceiling,
        byte_ceiling,
    );
    total.step(
        &mut root.events.original_work.visits_remaining,
        step_ceiling,
    );
    total.byte(&mut root.events.original_work.bytes_remaining, byte_ceiling);
    Ok(total)
}

fn abort_preserving_first<M: MerkleNodeReader>(
    transaction: &mut oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
) {
    let _: Result<(), ExecutionError> = transaction.contribute(|_| Err(ExecutionError::Aborted));
}

impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: oasis7_formal_protocol::incremental_material::OriginalObjectStreamReader,
{
    /// Run the original due material-transit phase against one parent-bound
    /// transaction. Per-item ordering stays reduce -> counted original/event ->
    /// route; a failure preserves its first cause and poisons this transaction.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn execute_due_material_transits_v1<P, CE, CC, AE, AC, IE, IC, ME>(
        &mut self,
        parent: &BoundNormalTickParentV1<'_>,
        materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
        current_read: &mut WorldFieldReadContextV1<'_, P, R, CE, CC>,
        agent_parent_read: &mut WorldFieldReadContextV1<'_, P, R, AE, AC>,
        industry_read: &mut IndustryFamilyReadContextV1<'_, M, R, IE, IC>,
        work: &mut crate::world::incremental_action_factory_lifecycle::FactoryLifecycleWorkV1,
        logistics_work: &mut LogisticsWorkBudgetV1,
        normal: &mut NormalTailWorkV1<'_, '_>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<usize, RootModuleExecutionErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        CE: FnMut() -> Option<ValueTreeError>,
        CC: FnMut() -> bool,
        AE: FnMut() -> Option<ValueTreeError>,
        AC: FnMut() -> bool,
        IE: FnMut() -> Option<ValueTreeError>,
        IC: FnMut() -> bool,
        ME: FnMut() -> Option<ValueTreeError>,
    {
        let result = self.execute_due_material_transits_inner_v1(
            parent,
            materials,
            current_read,
            agent_parent_read,
            industry_read,
            work,
            logistics_work,
            normal,
            directory,
            enumeration,
            live_owner,
        );
        if result.is_err() {
            abort_preserving_first(self.ctx.transaction);
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_due_material_transits_inner_v1<P, CE, CC, AE, AC, IE, IC, ME>(
        &mut self,
        parent: &BoundNormalTickParentV1<'_>,
        materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
        current_read: &mut WorldFieldReadContextV1<'_, P, R, CE, CC>,
        agent_parent_read: &mut WorldFieldReadContextV1<'_, P, R, AE, AC>,
        industry_read: &mut IndustryFamilyReadContextV1<'_, M, R, IE, IC>,
        work: &mut crate::world::incremental_action_factory_lifecycle::FactoryLifecycleWorkV1,
        logistics_work: &mut LogisticsWorkBudgetV1,
        normal: &mut NormalTailWorkV1<'_, '_>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<usize, RootModuleExecutionErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        CE: FnMut() -> Option<ValueTreeError>,
        CC: FnMut() -> bool,
        AE: FnMut() -> Option<ValueTreeError>,
        AC: FnMut() -> bool,
        IE: FnMut() -> Option<ValueTreeError>,
        IC: FnMut() -> bool,
        ME: FnMut() -> Option<ValueTreeError>,
    {
        let live = live_owner();
        let guard = self.ctx.transaction.guard();
        normal
            .charge(4096)
            .map_err(RootModuleExecutionErrorV1::Body)?;
        NormalTailSourcesV1::from_bound_parent(parent)
            .validate(
                self.ctx.transaction,
                self.events.mutation.memory,
                &guard,
                &live,
            )
            .map_err(|error| RootModuleExecutionErrorV1::Body(error.into()))?;
        normal
            .validate_parent(parent)
            .map_err(RootModuleExecutionErrorV1::Body)?;
        if !directory.uses_budget(self.events.mutation.memory)
            || !materials.parent.uses_budget(self.events.mutation.memory)
        {
            return Err(RootModuleExecutionErrorV1::Body(
                BodyMutationError::Invalid("normal transit driver source pool"),
            ));
        }

        let sources = NormalTailSourcesV1::from_bound_parent(parent);
        let roots = parent.refs().roots;
        let factories = sources.factory();
        let factory_index = sources.native().factory_lifecycle_index();
        let industry = sources.industry();
        let logistics = sources.native().logistics();
        let agents = sources.raw().agents();
        let native = sources.native().native();
        let memory = self.events.mutation.memory;
        let max_scalar_bytes = work.max_scalar_bytes;
        let max_items = work.pending_entries_remaining;

        // Selection is one bounded child. It emits only an ordered list of
        // candidate ids; it never publishes or authorizes a transit.
        let order = normal
            .with_child(|scoped, lease| {
                scoped.charge_record()?;
                let before = selection_lanes(
                    &mut work.industry,
                    industry_read,
                    current_read,
                    &mut self.events.mutation,
                    Some((lease.remaining_steps(), lease.remaining_read_bytes())),
                )?;
                let collected = (|| {
                    let guard = self.ctx.transaction.guard();
                    let mut step = IwsStepContext::borrow_counted_at(
                        self.ctx.transaction,
                        self.now,
                        &guard,
                        &live,
                        self.events.values,
                        &mut self.events.mutation,
                        &mut self.events.original_work,
                        self.events.world_events,
                    )?;
                    let result = crate::world::incremental_normal_tail::transits::collect_due_transit_order_v1(
                        roots,
                        industry,
                        industry_read,
                        &mut work.industry,
                        parent,
                        &mut step,
                        current_read,
                        memory,
                        scoped,
                        self.now,
                        max_scalar_bytes,
                        max_items,
                        &guard,
                        &live,
                    );
                    drop(step);
                    result
                })();
                let after = selection_lanes(
                    &mut work.industry,
                    industry_read,
                    current_read,
                    &mut self.events.mutation,
                    None,
                )?;
                let accounted = lane_delta(before, after, lease);
                match (collected, accounted) {
                    (Err(first), _) => Err(first),
                    (Ok(_), Err(accounting)) => Err(accounting),
                    (Ok(order), Ok(())) => Ok(order),
                }
            })
            .map_err(RootModuleExecutionErrorV1::Body)?;

        let mut routed_total = 0usize;
        for due in order.entries() {
            let live = live_owner();
            let prepared = normal
                .with_child(|scoped, lease| {
                    scoped.charge_record()?;
                    let before = reducer_lanes(
                        work,
                        logistics_work,
                        current_read,
                        agent_parent_read,
                        materials,
                        &mut self.events.mutation,
                        Some((lease.remaining_steps(), lease.remaining_read_bytes())),
                    )?;
                    let reduced = (|| {
                        let guard = self.ctx.transaction.guard();
                        let mut step = IwsStepContext::borrow_counted_at(
                            self.ctx.transaction,
                            self.now,
                            &guard,
                            &live,
                            self.events.values,
                            &mut self.events.mutation,
                            &mut self.events.original_work,
                            self.events.world_events,
                        )?;
                        let result = crate::world::incremental_normal_tail::transits::reduce_current_material_transit_v1(
                            due.job_id,
                            &mut step,
                            materials,
                            parent,
                            roots,
                            current_read,
                            agent_parent_read,
                            factories,
                            factory_index,
                            industry,
                            logistics,
                            agents,
                            native,
                            work,
                            logistics_work,
                            scoped,
                            &guard,
                            &live,
                        );
                        drop(step);
                        result
                    })();
                    let after = reducer_lanes(
                        work,
                        logistics_work,
                        current_read,
                        agent_parent_read,
                        materials,
                        &mut self.events.mutation,
                        None,
                    )?;
                    let accounted = lane_delta(before, after, lease);
                    match (reduced, accounted) {
                        (Err(first), _) => Err(first),
                        (Ok(_), Err(accounting)) => Err(accounting),
                        (Ok(prepared), Ok(())) => Ok(prepared),
                    }
                })
                .map_err(RootModuleExecutionErrorV1::Body)?;

            // Publish only the private reducer receipt, after the reducer child
            // has finished. System events route immediately, before next due id.
            let live = live_owner();
            let persisted = crate::world::incremental_module_execution_event::contribute_normal_material_transit_event_v1(
                self.now,
                &prepared,
                normal
                    .common_mut()
                    .map_err(RootModuleExecutionErrorV1::Body)?,
                &live,
                self.ctx.transaction,
                self.events,
                materials,
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;

            let mut route_result = None;
            let route_budget = normal.with_child(|scoped, lease| {
                scoped.charge_record()?;
                let before = route_lanes(
                    self,
                    enumeration,
                    Some((lease.remaining_steps(), lease.remaining_read_bytes())),
                )?;
                let result = self.route_persisted_event_v1(
                    &persisted,
                    None,
                    directory,
                    enumeration,
                    live_owner,
                );
                route_result = Some(result);
                let after = route_lanes(self, enumeration, None)?;
                let accounted = lane_delta(before, after, lease);
                match (&route_result, accounted) {
                    (Some(Err(_)), _) => Ok(()),
                    (Some(Ok(_)), Err(error)) => Err(error),
                    (Some(Ok(_)), Ok(())) => Ok(()),
                    (None, _) => Err(BodyMutationError::Invalid("normal transit route result")),
                }
            });
            route_budget.map_err(RootModuleExecutionErrorV1::Body)?;
            let routed = route_result.ok_or_else(|| {
                RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                    "normal transit route result",
                ))
            })??;
            routed_total =
                routed_total
                    .checked_add(routed)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "normal transit routed count",
                    ))?;
        }
        self.ctx
            .transaction
            .validate_guard(&self.ctx.transaction.guard(), &live_owner())?;
        Ok(routed_total)
    }
}
