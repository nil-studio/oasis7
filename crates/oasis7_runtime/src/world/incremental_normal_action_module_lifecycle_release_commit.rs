//! Actual Current release state publication. The private phase plan supplies
//! complete original before/after rows, never a caller-authored Domain event.
use super::event::{ModuleLifecycleCommittedStateV1, PreparedNormalModuleLifecycleEventV1};
use super::release_plan::{CurrentModuleReleaseMutationV1, PreparedCurrentModuleReleaseV1};
use super::*;
use crate::world::{
    incremental_action_execution::SelectedActionAuditV1,
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::incremental_value::{
    ValueNodeReader, ValueNodeWriter, ValueReadContext,
};
use oasis7_world_state::{
    incremental_field_member::{FieldDestination, MemberRecordWriteContext},
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
};

pub(super) struct CommittedCurrentModuleReleaseStateV1 {
    plan: PreparedCurrentModuleReleaseV1,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    now: u64,
    _fee: MemoryReservation,
}
impl CommittedCurrentModuleReleaseStateV1 {
    pub(super) fn event(&self) -> &crate::DomainEvent {
        &self.plan.event
    }
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        now: u64,
        core: WorkIdentityV1,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        self.plan
            .validate_evidence_v1(tx, memory, core, now, live)?;
        if self.now != now || self.core != core || !self._fee.uses_budget(memory) {
            return Err(invalid());
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn commit_current_module_release_v1<M, P, V, W, D, E, C, PE, PC, CW, ME, F>(
    plan: PreparedCurrentModuleReleaseV1,
    accepted: Option<SelectedActionAuditV1>,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    sources: &ModuleLifecycleSourcesV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    module: &mut ModuleLifecycleWorkV1<'_>,
    factory: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    max_event_bytes: usize,
    persist: &mut F,
    live: &ExecutionFence,
) -> Result<PreparedNormalModuleLifecycleEventV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader
        + ValueNodeWriter
        + oasis7_formal_protocol::incremental_state::StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let now = step.time();
        let state_fee = common
            .memory()
            .reserve(
                std::mem::size_of::<CommittedCurrentModuleReleaseStateV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        crate::world::incremental_step::factory_schedule_write_io::paid_with_module(
            step,
            read,
            parent,
            write,
            materials,
            factory,
            module,
            common,
            live,
            |step, read, parent, write, _materials, factory, module, work| {
                plan.validate_current_v1(selected, outcome, step.transaction, work, now, live)?;
                let mut ctx = WorldFieldOverlayContextV1 {
                    transaction: &mut *step.transaction,
                    values: ValueReadContext {
                        source: read.values.source,
                        memory: read.values.memory,
                        class: read.values.class,
                        io: &mut *read.values.io,
                    },
                    key_work: &mut *read.key_work,
                    semantic_bytes: read.semantic_bytes,
                    record_bytes: read.record_bytes,
                    take_state_error: &mut read.take_state_error,
                    cancelled: &mut read.cancelled,
                };
                match &plan.mutation {
                    CurrentModuleReleaseMutationV1::Request {
                        outer,
                        before_request,
                        after_request,
                        before_mapping,
                        after_mapping,
                        next,
                    } => {
                        let descriptors=oasis7_world_state::incremental_world_native::world_native_field_descriptors_v1()?;
                        work.debit(48, 0)?;
                        let namespace = |name| -> Result<_, BodyMutationError> {
                            let descriptor = descriptors
                                .iter()
                                .find(|field| field.name == name)
                                .ok_or_else(invalid)?;
                            let FieldDestination::State(namespace) = descriptor.destination else {
                                return Err(invalid());
                            };
                            Ok(namespace)
                        };
                        {
                            let mut native = WorldFieldReadContextV1 {
                                root: sources
                                    .families
                                    .roots
                                    .state_root(namespace("module_release_requests")?),
                                state: parent.state,
                                values: ValueReadContext {
                                    source: ctx.values.source,
                                    memory: ctx.values.memory,
                                    class: ctx.values.class,
                                    io: &mut *ctx.values.io,
                                },
                                key_work: &mut *ctx.key_work,
                                semantic_bytes: ctx.semantic_bytes,
                                record_bytes: ctx.record_bytes,
                                take_state_error: &mut ctx.take_state_error,
                                cancelled: &mut ctx.cancelled,
                            };
                            oasis7_world_state::incremental_world_native::write_current_module_release_request_v1(before_request,*outer,after_request,ctx.transaction,&mut native,write,&mut factory.lifecycle.records_remaining,&mut factory.lifecycle.native_write,live).map_err(overlay_error)?;
                        }
                        if let Some(after) = after_mapping {
                            let mut native = WorldFieldReadContextV1 {
                                root: sources
                                    .families
                                    .roots
                                    .state_root(namespace("module_release_manifest_mappings")?),
                                state: parent.state,
                                values: ValueReadContext {
                                    source: ctx.values.source,
                                    memory: ctx.values.memory,
                                    class: ctx.values.class,
                                    io: &mut *ctx.values.io,
                                },
                                key_work: &mut *ctx.key_work,
                                semantic_bytes: ctx.semantic_bytes,
                                record_bytes: ctx.record_bytes,
                                take_state_error: &mut ctx.take_state_error,
                                cancelled: &mut ctx.cancelled,
                            };
                            oasis7_world_state::incremental_world_native::write_current_module_release_manifest_mapping_v1(before_mapping,*outer,after,ctx.transaction,&mut native,write,&mut factory.lifecycle.records_remaining,&mut factory.lifecycle.native_write,live).map_err(overlay_error)?;
                        }
                        if let Some((before, after)) = next {
                            let guard = ctx.transaction.guard();
                            oasis7_world_state::incremental_world_u64::change_world_u64_overlay_v1(oasis7_world_state::incremental_world_u64::WorldU64FieldV1::NextModuleReleaseRequestId,*before,*after,&mut ctx,write,&guard,live).map_err(overlay_error)?;
                        }
                    }
                    CurrentModuleReleaseMutationV1::Roles { before, after } => {
                        let guard = ctx.transaction.guard();
                        oasis7_world_state::incremental_world_collection_controls::replace_current_module_release_roles_v1(before,after,&mut ctx,write,module.controls,module.pending_entries,&guard,live).map_err(overlay_error)?;
                    }
                }
                // Release projection changes only activity, never an unrelated resource
                // balance. Distinct BindRoles target and operator retain their own sources.
                for agent in plan.agents.iter().flatten() {
                    super::commit::write_market_agent_v1(
                        agent,
                        &agent.stock,
                        now,
                        sources,
                        &mut ctx,
                        write,
                        live,
                    )?;
                }
                Ok(())
            },
        )?;
        let heap = plan
            .event
            .reserved_bytes()
            .checked_add(std::mem::size_of::<crate::WorldEvent>())
            .ok_or_else(invalid)?;
        pay(common, heap)?;
        let fee = common
            .memory()
            .reserve(heap, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let body = (*plan.event).clone();
        let id = {
            let mut ctx = WorldFieldOverlayContextV1 {
                transaction: &mut *step.transaction,
                values: ValueReadContext {
                    source: read.values.source,
                    memory: read.values.memory,
                    class: read.values.class,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: &mut read.cancelled,
            };
            crate::world::incremental_normal_tail::event_preview::preview_current_normal_event_id_v1(&mut ctx,common)?
        };
        let action_id = plan.action_id();
        let state = CommittedCurrentModuleReleaseStateV1 {
            plan,
            guard: step.transaction.guard(),
            core: common.work().identity(),
            now,
            _fee: state_fee,
        };
        let layout = common
            .memory()
            .reserve(
                std::mem::size_of::<PreparedNormalModuleLifecycleEventV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let cap = PreparedNormalModuleLifecycleEventV1 {
            event: fee.retain(crate::WorldEvent {
                id,
                time: now,
                caused_by: Some(crate::CausedBy::Action(action_id)),
                body: crate::WorldEventBody::Domain(body),
            }),
            accepted,
            guard: step.transaction.guard(),
            core: common.work().identity(),
            action_id,
            memory: common.memory().clone(),
            state: ModuleLifecycleCommittedStateV1::Release(state),
            _layout: layout,
            _prepare_fees: std::array::from_fn(|_| None),
        };
        step.append_prepared_module_lifecycle_event_v1(
            &cap,
            materials,
            common,
            live,
            max_event_bytes,
            persist,
        )?;
        Ok(cap)
    }));
    let result = match attempt {
        Ok(result) => result,
        Err(payload) => {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                step.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
            std::panic::resume_unwind(payload)
        }
    };
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            step.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}
