//! One original due-Tick selection cut. Slots cannot be built from a caller
//! manifest/flag; they are consumed only by this module's fixed route loop.
use super::*;
use crate::world::incremental_module_invocation::*;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_material::OriginalObjectStreamReader,
    incremental_schedule::SchedulePageWriter,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use std::sync::Arc;
fn bad() -> ValueTreeError {
    ValueTreeError::Invalid("original gameplay Tick route selection")
}
fn selection_error<OE>(
    e: ModuleInvocationPreparationErrorV1<std::convert::Infallible>,
) -> RootModuleExecutionErrorV1<OE> {
    match e{ModuleInvocationPreparationErrorV1::Body(e)=>RootModuleExecutionErrorV1::Body(e),ModuleInvocationPreparationErrorV1::Value(e)=>RootModuleExecutionErrorV1::Value(e),ModuleInvocationPreparationErrorV1::Overlay(e)=>RootModuleExecutionErrorV1::Body(match e{oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Execution(e)=>e.into(),oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(e)=>e.into(),oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Member(e)=>BodyMutationError::Original(Box::new(e))}),ModuleInvocationPreparationErrorV1::Original(_)=>unreachable!("manifest-only fixed selector never opens originals")}
}
#[derive(Clone)]
pub(in crate::world) struct CapturedGameplayWorldHashV1 {
    owner: Arc<ReservedValue<ReservedValue<CurrentWorldManifestHashV1>>>,
}
impl CapturedGameplayWorldHashV1 {
    pub(in crate::world) fn hash_owner(&self) -> &ReservedValue<CurrentWorldManifestHashV1> {
        &self.owner
    }
}
pub(in crate::world) struct DueGameplayTickSlotV1 {
    instance: String,
    selection: Option<CurrentModuleManifestSelectionV1>,
    hash: Option<CapturedGameplayWorldHashV1>,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    _heap: Arc<ReservedValue<()>>,
}
impl DueGameplayTickSlotV1 {
    pub(in crate::world) fn instance_id(&self) -> &str {
        &self.instance
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        core: WorkIdentityV1,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.core != core
            || !self.memory.same_pool(memory)
            || !self._heap.uses_budget(memory)
            || !tx.uses_budget(memory)
        {
            return Err(BodyMutationError::Invalid(
                "captured Tick selection identity/Core/pool",
            ));
        }
        Ok(())
    }
    pub(in crate::world) fn into_selection(
        self,
    ) -> (
        Option<CurrentModuleManifestSelectionV1>,
        Option<CapturedGameplayWorldHashV1>,
    ) {
        (self.selection, self.hash)
    }
}
pub(super) struct DueGameplayTickRouteV1 {
    slots: std::vec::IntoIter<DueGameplayTickSlotV1>,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    has_gameplay_tick: bool,
    _heap: Arc<ReservedValue<()>>,
}
impl DueGameplayTickRouteV1 {
    pub(super) fn has_gameplay_tick(&self) -> bool {
        self.has_gameplay_tick
    }
}
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter + SchedulePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    pub(super) fn gameplay_manifest_selection_v1(
        &mut self,
        id: &str,
        registry_only: bool,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<Option<CurrentModuleManifestSelectionV1>, RootModuleExecutionErrorV1<O::Error>>
    {
        // registry_only selects an original DATA lookup route, never permission.
        with_current_root_module_meter_v1(self, common, live_owner, |root, live_owner| {
            let live = live_owner();
            let guard = root.ctx.transaction.guard();
            let request = ModuleInvocationPreparationRequestV1 {
                invocation_id: id,
                parent_roots: root.families.roots,
                instances: root.families.instances,
                config: root.families.config,
                content: root.families.content,
                current_active: None,
                current_record: None,
                guard: &guard,
                live: &live,
            };
            (if registry_only {
                read_current_registry_module_manifest_selection_v1(
                    request,
                    root.state,
                    root.ctx,
                    &mut root.work.selection_scan,
                    &mut root.work.selection,
                    &mut root.work.selection_limits,
                )
            } else {
                read_current_root_module_manifest_selection_v1(
                    request,
                    root.state,
                    root.ctx,
                    &mut root.work.selection_scan,
                    &mut root.work.selection,
                    &mut root.work.selection_limits,
                )
            })
            .map_err(selection_error)
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_gameplay_tick_route_v1<P>(
        &mut self,
        state: &P,
        work: &mut NormalGameplayCycleWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<DueGameplayTickRouteV1, RootModuleExecutionErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
    {
        let live = live_owner();
        common.consume_steps(1).map_err(|e| {
            RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
        })?;
        let limit = common.work().remaining_steps() / 8;
        let bytes_limit = common.work().remaining_read_bytes() / 4;
        enumeration.instance_scan.nodes_remaining =
            enumeration.instance_scan.nodes_remaining.min(limit);
        enumeration.instance_scan.leaves_remaining =
            enumeration.instance_scan.leaves_remaining.min(limit);
        enumeration.registry_scan.nodes_remaining =
            enumeration.registry_scan.nodes_remaining.min(limit);
        enumeration.registry_scan.leaves_remaining =
            enumeration.registry_scan.leaves_remaining.min(limit);
        enumeration.pending_entries = enumeration
            .pending_entries
            .min(usize::try_from(limit).unwrap_or(usize::MAX));
        enumeration.instance_scan.read_bytes_remaining = enumeration
            .instance_scan
            .read_bytes_remaining
            .min(bytes_limit);
        enumeration.registry_scan.read_bytes_remaining = enumeration
            .registry_scan
            .read_bytes_remaining
            .min(bytes_limit);
        enumeration.comparison_bytes_remaining =
            enumeration.comparison_bytes_remaining.min(bytes_limit);
        let before = [
            enumeration.instance_scan.nodes_remaining,
            enumeration.instance_scan.leaves_remaining,
            enumeration.registry_scan.nodes_remaining,
            enumeration.registry_scan.leaves_remaining,
            enumeration.pending_entries as u64,
            enumeration.instance_scan.read_bytes_remaining,
            enumeration.registry_scan.read_bytes_remaining,
            enumeration.comparison_bytes_remaining,
        ];
        let ids = with_current_root_module_meter_v1(
            self,
            common,
            live_owner,
            |root, live_owner| {
                let guard = root.ctx.transaction.guard();
                let live = live_owner();
                collect_current_module_invocation_ids_v1(directory,root.families.instances,root.families.config,state,root.ctx,enumeration,&guard,&live).map_err(|e|match e{CurrentInvocationEnumerationErrorV1::Execution(e)=>RootModuleExecutionErrorV1::Body(e.into()),CurrentInvocationEnumerationErrorV1::Value(e)=>RootModuleExecutionErrorV1::Value(e),CurrentInvocationEnumerationErrorV1::Body(e)=>RootModuleExecutionErrorV1::Body(e),CurrentInvocationEnumerationErrorV1::Overlay(e)=>RootModuleExecutionErrorV1::Body(match e{oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Execution(e)=>e.into(),oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(e)=>e.into(),oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Member(e)=>BodyMutationError::Original(Box::new(e))})})
            },
        );
        let after = [
            enumeration.instance_scan.nodes_remaining,
            enumeration.instance_scan.leaves_remaining,
            enumeration.registry_scan.nodes_remaining,
            enumeration.registry_scan.leaves_remaining,
            enumeration.pending_entries as u64,
            enumeration.instance_scan.read_bytes_remaining,
            enumeration.registry_scan.read_bytes_remaining,
            enumeration.comparison_bytes_remaining,
        ];
        let steps = (0..5)
            .try_fold(0u64, |n, i| n.checked_add(before[i].checked_sub(after[i])?))
            .ok_or_else(bad)?;
        let bytes = (5..8)
            .try_fold(0u64, |n, i| n.checked_add(before[i].checked_sub(after[i])?))
            .ok_or_else(bad)?;
        let accounting = common.charge(steps, bytes);
        let ids = ids?;
        accounting.map_err(|e| {
            RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
        })?;
        let mut has_gameplay_tick = false;
        // Original collect_active_module_invocations completes every manifest lookup
        // before its any() predicate. Do not short circuit physical source reads.
        for id in ids.iter() {
            if let Some(selected) =
                self.gameplay_manifest_selection_v1(id, false, common, live_owner)?
            {
                if selected.manifest().role == crate::ModuleRole::Gameplay
                    && selected.manifest().subscriptions.iter().any(|s| {
                        s.resolved_stage() == oasis7_wasm_abi::ModuleSubscriptionStage::Tick
                    })
                {
                    has_gameplay_tick = true;
                }
            }
        }
        // Due selection itself is delegated to the original current scalar/index
        // reader. Its actual IO runs under the same Root/Common meter.
        let step_limit = common.work().remaining_steps() / 4;
        let byte_limit = common.work().remaining_read_bytes() / 2;
        work.due_scan.nodes_remaining = work.due_scan.nodes_remaining.min(step_limit);
        work.due_scan.buckets_remaining = work.due_scan.buckets_remaining.min(step_limit);
        work.due_scan.instances_remaining = work.due_scan.instances_remaining.min(step_limit);
        work.due_scan.read_bytes_remaining = work.due_scan.read_bytes_remaining.min(byte_limit);
        let before = [
            work.due_scan.nodes_remaining,
            work.due_scan.buckets_remaining,
            work.due_scan.instances_remaining,
            work.due_scan.read_bytes_remaining,
        ];
        let due = with_current_root_module_meter_v1(self, common, live_owner, |root, _| {
            crate::world::World::read_iws_due_module_batch_at(
                root.now,
                root.ctx.transaction,
                root.events.values,
                &mut root.events.mutation,
                &mut work.due_scan,
                work.due_limits,
            )
            .map_err(RootModuleExecutionErrorV1::Body)
        });
        let after = [
            work.due_scan.nodes_remaining,
            work.due_scan.buckets_remaining,
            work.due_scan.instances_remaining,
            work.due_scan.read_bytes_remaining,
        ];
        let steps = (0..3)
            .try_fold(0u64, |n, i| n.checked_add(before[i].checked_sub(after[i])?))
            .ok_or_else(bad)?;
        let bytes = before[3].checked_sub(after[3]).ok_or_else(bad)?;
        let charged = common.charge(steps, bytes);
        let due = due?;
        charged.map_err(|e| {
            RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
        })?;
        self.ctx
            .transaction
            .validate_guard(due.guard(), &live_owner())?;
        let hash = if due.iter().len() == 0 {
            None
        } else {
            let hash = with_current_root_module_meter_v1(self, common, live_owner, |root, _| {
                prepare_current_world_manifest_hash_v1(
                    root.families.parent,
                    root.families.head,
                    root.families.roots,
                    root.manifest_cache,
                    None,
                    root.families.config,
                    root.state,
                    root.ctx,
                    root.manifest_work,
                )
                .map_err(RootModuleExecutionErrorV1::Manifest)
            })?;
            let fee = common
                .memory()
                .reserve(
                    std::mem::size_of::<ReservedValue<ReservedValue<CurrentWorldManifestHashV1>>>()
                        + 2 * std::mem::size_of::<usize>()
                        + 2 * std::mem::align_of::<
                            ReservedValue<ReservedValue<CurrentWorldManifestHashV1>>,
                        >(),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)?;
            Some(CapturedGameplayWorldHashV1 {
                owner: Arc::new(fee.retain(hash)),
            })
        };
        let count = due.iter().len();
        let strings = due
            .iter()
            .try_fold(0usize, |n, (id, _)| n.checked_add(id.len()).ok_or_else(bad))?;
        let metadata = count
            .checked_mul(std::mem::size_of::<DueGameplayTickSlotV1>())
            .and_then(|n| n.checked_add(strings))
            .and_then(|n| {
                n.checked_add(
                    std::mem::size_of::<ReservedValue<()>>()
                        + 4 * std::mem::size_of::<usize>()
                        + 4 * std::mem::align_of::<ReservedValue<()>>(),
                )
            })
            .ok_or_else(bad)?;
        common.charge(count as u64, metadata as u64).map_err(|e| {
            RootModuleExecutionErrorV1::Body(BodyMutationError::Original(Box::new(e)))
        })?;
        let fee = common
            .memory()
            .reserve(metadata, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let heap = Arc::new(fee.retain(()));
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(count)
            .map_err(|_| ValueTreeError::ResourceLimited("gameplay Tick slots"))?;
        if slots.capacity() > count {
            return Err(bad().into());
        }
        for (id, _) in due.iter() {
            let selection = self.gameplay_manifest_selection_v1(id, false, common, live_owner)?;
            slots.push(DueGameplayTickSlotV1 {
                instance: id.to_owned(),
                selection,
                hash: hash.clone(),
                guard: self.ctx.transaction.guard(),
                core: common.work().identity(),
                memory: common.memory().clone(),
                _heap: heap.clone(),
            });
        }
        let guard = self.ctx.transaction.guard();
        self.ctx.transaction.validate_guard(&guard, &live_owner())?;
        Ok(DueGameplayTickRouteV1 {
            slots: slots.into_iter(),
            guard,
            core: common.work().identity(),
            memory: common.memory().clone(),
            has_gameplay_tick,
            _heap: heap,
        })
    }
    pub(super) fn execute_gameplay_tick_route_v1(
        &mut self,
        mut route: DueGameplayTickRouteV1,
        common: &mut FormalBoundedContextV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<(), RootModuleExecutionErrorV1<O::Error>> {
        self.ctx
            .transaction
            .validate_guard(&route.guard, &live_owner())?;
        if route.core != common.work().identity()
            || !route.memory.same_pool(common.memory())
            || !route._heap.uses_budget(common.memory())
        {
            return Err(RootModuleExecutionErrorV1::Body(
                BodyMutationError::Invalid("gameplay Tick route Core/pool"),
            ));
        }
        // All manifests above were resolved before the first VM. Each actual due
        // owner removes its old wake before using the captured slot and original
        // staged-state VM path; subsequent slots retain the same transaction only.
        for slot in route.slots.by_ref() {
            let core = common.work().identity();
            with_current_root_module_meter_v1(self, common, live_owner, move |root, live| {
                root.execute_captured_scheduled_gameplay_tick_v1(slot, core, live)
            })?;
        }
        self.ctx
            .transaction
            .validate_guard(&self.ctx.transaction.guard(), &live_owner())?;
        Ok(())
    }
}
