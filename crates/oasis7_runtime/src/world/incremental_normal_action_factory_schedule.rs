//! Fixed Current BuildFactory/ScheduleRecipe owner. All state effects precede
//! the private counted-event capability; ordinary Domain dispatch cannot mint it.
use crate::world::{
    incremental_action_execution::{SelectedActionAuditV1, SelectedPendingActionV1},
    incremental_body::BodyMutationError,
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_module_invocation::{RootRulePhaseOutcomeV1, SelectedAcceptedDispositionV1},
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{
        ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
    },
    incremental_state::{MerkleNodeReader, StatePageWriter, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_event_decode::WorldEventDecodeWorkV1,
};
use std::mem::size_of;
#[path = "incremental_normal_action_factory_schedule_bounds.rs"]
mod bounds;
#[path = "incremental_normal_action_factory_schedule_commit.rs"]
mod commit;
#[path = "incremental_normal_action_factory_schedule_current.rs"]
mod current;
#[path = "incremental_normal_action_factory_schedule_io.rs"]
mod io;
#[path = "incremental_normal_action_factory_schedule_modules.rs"]
mod modules;
#[path = "incremental_normal_action_factory_schedule_recipe_current.rs"]
mod recipe_current;
#[path = "incremental_normal_action_factory_schedule_event.rs"]
mod record;
#[path = "incremental_normal_action_factory_schedule_starter.rs"]
mod starter;

pub(in crate::world) struct FactoryScheduleSourcesV1<'a>{
    pub roots:&'a RootSetV1,
    pub lifecycle:crate::world::incremental_action_factory_lifecycle::FactoryLifecycleSourcesV1<'a>,
    pub builds:&'a ReservedValue<oasis7_world_state::incremental_factory_schedule_index::FactoryScheduleIndexV1>,
    pub receipts:&'a ReservedValue<oasis7_world_state::incremental_factory_schedule_receipts::FactoryScheduleReceiptIndexV1>,
    pub inventory_directory:&'a ReservedValue<oasis7_world_state::incremental_inventory::InventoryIdentityDirectoryV1>,
    pub factory_authority:&'a ReservedValue<oasis7_world_state::incremental_factory_authority_records::FactoryAuthorityFamilyQualificationV1>,
    pub agent_authority:&'a ReservedValue<oasis7_world_state::incremental_agent_authority_records::AgentAuthorityFamilyQualificationV1>,
    pub logistics:&'a ReservedValue<oasis7_world_state::incremental_logistics_records::LogisticsFamilyQualificationV1>,
    pub material_profiles:&'a ReservedValue<oasis7_world_state::incremental_material_records::MaterialFamilyQualificationV1>,
    pub artifact_config:&'a ReservedValue<oasis7_world_state::incremental_runtime_artifact_config::ArtifactConfigFamilyQualificationV1>,
    pub module_instances:&'a ReservedValue<oasis7_world_state::incremental_module_instance_family::ModuleInstanceFamilyQualificationV1>,
    pub invocation_directory:&'a ReservedValue<crate::world::incremental_module_invocation::AcceptedInvocationDirectoryV1>,
}
pub(in crate::world) struct FactoryScheduleWorkV1 {
    pub pending_entries_remaining: usize,
    pub lifecycle: crate::world::incremental_action_factory_lifecycle::FactoryLifecycleWorkV1,
    pub builds: oasis7_world_state::incremental_factory_schedule_index::FactoryScheduleIndexWorkV1,
    pub receipts:
        oasis7_world_state::incremental_factory_schedule_receipts::FactoryScheduleReceiptWorkV1,
    pub authority:
        oasis7_world_state::incremental_factory_schedule_authority::FactoryScheduleAuthorityWorkV1,
    pub factory:
        oasis7_world_state::incremental_factory_schedule_factory::FactoryScheduleFactoryWorkV1,
    pub authority_scan: StateScanBudget,
    pub authority_cells: u64,
    pub max_source_records: usize,
    pub max_source_encoding_bytes: usize,
    pub max_module_text_bytes: usize,
    pub logistics:
        oasis7_world_state::incremental_factory_schedule_logistics::FactoryScheduleLogisticsWorkV1,
    pub logistics_write: oasis7_world_state::incremental_logistics_records::LogisticsWorkBudgetV1,
    pub material_profiles:
        oasis7_world_state::incremental_material_records::MaterialProfileCurrentReadWorkV1,
    pub invocation: crate::world::incremental_module_invocation::CurrentInvocationEnumerationWorkV1,
    pub config: oasis7_world_state::incremental_runtime_artifact_config::ArtifactConfigReadWorkV1,
    pub manifest:
        oasis7_world_state::incremental_runtime_artifact_config::SelectedModuleManifestLimitsV1,
    pub config_scan: StateScanBudget,
    pub inventory_directory:
        oasis7_world_state::incremental_inventory::InventoryIdentityDirectoryWorkV1,
    pub inventory_family: oasis7_world_state::incremental_inventory::InventoryFamilyWorkV1,
}
fn invalid() -> BodyMutationError {
    BodyMutationError::Invalid("factory schedule Current owner")
}
fn limited() -> BodyMutationError {
    ValueTreeError::ResourceLimited("factory schedule owner layout").into()
}
fn charge(common: &mut FormalBoundedContextV1<'_>, n: usize) -> Result<(), BodyMutationError> {
    common
        .charge(1, u64::try_from(n).map_err(|_| limited())?)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))
}
fn overlay(error: WorldFieldOverlayErrorV1) -> BodyMutationError {
    match error {
        WorldFieldOverlayErrorV1::Execution(e) => e.into(),
        WorldFieldOverlayErrorV1::Value(e) => e.into(),
        WorldFieldOverlayErrorV1::Member(e) => BodyMutationError::Original(Box::new(e)),
    }
}

/// Only the successful Current committer may construct this type.
pub(in crate::world) struct PreparedFactoryScheduleEventV1 {
    event: ReservedValue<crate::WorldEvent>,
    accepted: Option<SelectedActionAuditV1>,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    memory: MemoryBudget,
    _phase: MemoryReservation,
    _source_fees: current::SourceFeesV1,
}
impl PreparedFactoryScheduleEventV1 {
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        &self.event
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
        identity: WorkIdentityV1,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        if let Some(accepted) = &self.accepted {
            accepted.validate(tx)?;
        }
        if !self.memory.same_pool(memory)
            || !self.event.uses_budget(memory)
            || !tx.uses_budget(memory)
            || self.core != identity
            || self.event.time != now
            || self
                .accepted
                .as_ref()
                .is_some_and(|audit| self.event.caused_by != audit.event().caused_by)
        {
            return Err(invalid());
        }
        match &self.event.body {
            crate::WorldEventBody::Domain(
                crate::DomainEvent::FactoryBuildStarted { job_id, .. }
                | crate::DomainEvent::RecipeStarted { job_id, .. },
            ) if self.event.caused_by == Some(crate::CausedBy::Action(*job_id)) => Ok(()),
            _ => Err(invalid()),
        }
    }
}
pub(in crate::world) enum CompletedFactoryScheduleActionV1 {
    Applied(PreparedFactoryScheduleEventV1),
    RuleRejected {
        accepted: Option<SelectedActionAuditV1>,
        event: SelectedActionAuditV1,
        core: WorkIdentityV1,
        memory: MemoryBudget,
        _layout: ReservedValue<()>,
    },
    Rejected {
        accepted: Option<SelectedActionAuditV1>,
        event: SelectedActionAuditV1,
        core: WorkIdentityV1,
        memory: MemoryBudget,
        _phase: MemoryReservation,
        _source_fees: current::SourceFeesV1,
    },
}
impl CompletedFactoryScheduleActionV1 {
    pub(in crate::world) fn validate_transaction<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
    ) -> Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> {
        match self {
            Self::Applied(cap) => {
                tx.validate_transaction_identity(&cap.guard)?;
                match &cap.accepted {
                    Some(audit) => audit.validate(tx),
                    None => Ok(()),
                }
            }
            Self::Rejected {
                accepted, event, ..
            }
            | Self::RuleRejected {
                accepted, event, ..
            } => {
                if let Some(audit) = accepted {
                    audit.validate(tx)?;
                }
                event.validate(tx)
            }
        }
    }

    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        match self {
            Self::Applied(cap) => cap.event(),
            Self::Rejected { event, .. } | Self::RuleRejected { event, .. } => event.event(),
        }
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        now: u64,
        identity: WorkIdentityV1,
    ) -> Result<(), BodyMutationError> {
        match self {
            Self::Applied(cap) => cap.validate(tx, memory, now, identity),
            Self::Rejected {
                accepted,
                event,
                core,
                memory: pool,
                ..
            }
            | Self::RuleRejected {
                accepted,
                event,
                core,
                memory: pool,
                ..
            } => {
                if let Some(audit) = accepted {
                    audit.validate(tx)?;
                }
                event.validate(tx)?;
                if *core != identity
                    || !pool.same_pool(memory)
                    || !tx.uses_budget(memory)
                    || event.event().time != now
                    || accepted
                        .as_ref()
                        .is_some_and(|audit| audit.event().caused_by != event.event().caused_by)
                {
                    return Err(invalid());
                }
                Ok(())
            }
        }
    }
}

/// Preparation authenticates only source/Rule/transaction bindings and pays the
/// holder layout. Business evaluation runs once AFTER the genuine Accepted fee
/// is applied, exactly like the original World step.
pub(in crate::world) struct PreparedFactoryScheduleActionV1<'s, 'r, 'source> {
    selected: &'s SelectedPendingActionV1,
    outcome: &'r ReservedValue<RootRulePhaseOutcomeV1>,
    sources: &'s FactoryScheduleSourcesV1<'source>,
    guard: PreparedGuard,
    identity: WorkIdentityV1,
    memory: MemoryBudget,
    now: u64,
    event_fee: MemoryReservation,
    phase: MemoryReservation,
}
struct EvaluationWorldErrorV1 {
    cause: crate::WorldError,
    _fees: current::SourceFeesV1,
}
impl std::fmt::Debug for EvaluationWorldErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EvaluationWorldErrorV1")
            .field("cause", &self.cause)
            .finish()
    }
}
impl std::fmt::Display for EvaluationWorldErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.cause)
    }
}
impl std::error::Error for EvaluationWorldErrorV1 {}
fn evaluation_error(
    error: current::EvaluationErrorV1,
    fees: current::SourceFeesV1,
) -> BodyMutationError {
    match error {
        current::EvaluationErrorV1::Body(error) => error,
        current::EvaluationErrorV1::World(cause) => {
            BodyMutationError::Original(Box::new(EvaluationWorldErrorV1 { cause, _fees: fees }))
        }
    }
}
fn is_action(action: &crate::Action) -> bool {
    matches!(
        action,
        crate::Action::BuildFactory { .. } | crate::Action::ScheduleRecipe { .. }
    )
}
fn source_binding<M: MerkleNodeReader>(
    sources: &FactoryScheduleSourcesV1<'_>,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<(), BodyMutationError> {
    let memory = common.memory().clone();
    if !tx.uses_budget(&memory)
        || !sources.lifecycle.agents.uses_budget(&memory)
        || !sources.lifecycle.factories.uses_budget(&memory)
        || !sources.lifecycle.industry.uses_budget(&memory)
        || !sources.lifecycle.index.uses_budget(&memory)
        || !sources.lifecycle.inventory.uses_budget(&memory)
        || !sources.lifecycle.primitive.uses_budget(&memory)
        || !sources.lifecycle.native.uses_budget(&memory)
        || !sources.builds.uses_budget(&memory)
        || !sources.receipts.uses_budget(&memory)
        || !sources.inventory_directory.uses_budget(&memory)
        || !sources.factory_authority.uses_budget(&memory)
        || !sources.agent_authority.uses_budget(&memory)
        || !sources.logistics.uses_budget(&memory)
        || !sources.material_profiles.uses_budget(&memory)
        || !sources.artifact_config.uses_budget(&memory)
        || !sources.module_instances.uses_budget(&memory)
        || !sources.invocation_directory.uses_budget(&memory)
    {
        return Err(invalid());
    }
    // Fixed aggregate validation: each real native slot validates its roots once;
    // the remaining thirteen paths are the source's root and distinct family/
    // joint-index bindings listed immediately below. A single 4096-byte transient
    // admission spans the original bounded RootSet encodings.
    let paths = oasis7_world_state::incremental_world_native::world_native_field_descriptors_v1()?
        .len()
        .checked_add(13)
        .ok_or_else(limited)?;
    let n = sources
        .roots
        .world_id
        .len()
        .checked_add(1024 + 6 * oasis7_formal_protocol::incremental_state::STATE_ROOT_BYTES)
        .and_then(|n| n.checked_mul(paths))
        .ok_or_else(limited)?;
    common
        .charge(
            u64::try_from(paths).map_err(|_| limited())?,
            u64::try_from(n).map_err(|_| limited())?,
        )
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    let _phase = memory
        .reserve(4096, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let digest = sources.roots.digest()?;
    if digest != tx.parent_fence().parent_roots {
        return Err(invalid());
    }
    sources.lifecycle.agents.validate_binding(sources.roots)?;
    sources.lifecycle.factories.validate_binding(&digest)?;
    sources.lifecycle.industry.validate_binding(&digest)?;
    sources.lifecycle.index.validate_binding(
        sources.lifecycle.factories,
        sources.lifecycle.industry,
        sources.roots,
        &memory,
        MemoryClass::Ordinary,
    )?;
    sources
        .lifecycle
        .inventory
        .validate_binding(sources.roots)?;
    sources.lifecycle.native.validate_binding(sources.roots)?;
    sources
        .lifecycle
        .primitive
        .validate_binding(sources.roots)
        .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
    sources.builds.validate_binding(
        sources.lifecycle.factories,
        sources.roots,
        &memory,
        MemoryClass::Ordinary,
    )?;
    sources.receipts.validate_binding(
        sources.lifecycle.industry,
        sources.roots,
        &memory,
        MemoryClass::Ordinary,
    )?;
    sources
        .inventory_directory
        .validate_binding(sources.roots)?;
    sources.factory_authority.validate_binding(&digest)?;
    sources.agent_authority.validate_binding(&digest)?;
    sources.logistics.validate_binding(&digest)?;
    sources.material_profiles.validate_binding(&digest)?;
    sources.artifact_config.validate_binding(&digest)?;
    Ok(())
}
pub(in crate::world) fn prepare_factory_schedule_action_v1<'s, 'r, 'source, M, V, W, D>(
    selected: &'s SelectedPendingActionV1,
    outcome: &'r ReservedValue<RootRulePhaseOutcomeV1>,
    sources: &'s FactoryScheduleSourcesV1<'source>,
    step: &IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<PreparedFactoryScheduleActionV1<'s, 'r, 'source>, BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    D: FnMut() -> bool,
{
    charge(common, 0)?;
    selected.validate(step.transaction)?;
    if !outcome.uses_budget(common.memory())
        || !step.mutation.memory.same_pool(common.memory())
        || outcome.decision.verdict == crate::RuleVerdict::Deny
    {
        return Err(invalid());
    }
    let action = outcome
        .override_action_for_selected_v1(selected, step.transaction)?
        .unwrap_or(&selected.envelope().action);
    if !is_action(action) {
        return Err(invalid());
    }
    source_binding(sources, step.transaction, common)?;
    let event_fee = common
        .memory()
        .reserve(size_of::<crate::WorldEvent>(), MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let phase = common
        .memory()
        .reserve(
            size_of::<PreparedFactoryScheduleActionV1<'s, 'r, 'source>>()
                .checked_add(size_of::<CompletedFactoryScheduleActionV1>())
                .and_then(|n| n.checked_add(size_of::<PreparedFactoryScheduleEventV1>()))
                .ok_or_else(limited)?,
            MemoryClass::Ordinary,
        )
        .map_err(ValueTreeError::Memory)?;
    Ok(PreparedFactoryScheduleActionV1 {
        selected,
        outcome,
        sources,
        guard: step.transaction.guard(),
        identity: common.work().identity(),
        memory: common.memory().clone(),
        now: step.time(),
        event_fee,
        phase,
    })
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn commit_factory_schedule_action_v1<M, P, V, W, D, E, C, PE, PC, CW, ME, F>(
    prepared: PreparedFactoryScheduleActionV1<'_, '_, '_>,
    accepted: SelectedAcceptedDispositionV1,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    work: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
    max_event_bytes: usize,
    audit: &mut WorldEventDecodeWorkV1,
    persist: &mut F,
) -> Result<CompletedFactoryScheduleActionV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let result = (|| {
        charge(common, 0)?;
        step.transaction
            .validate_transaction_identity(&prepared.guard)?;
        accepted.validate(prepared.selected, step.transaction, step.mutation.memory)?;
        if prepared.identity != common.work().identity()
            || !prepared.memory.same_pool(common.memory())
            || !step.mutation.memory.same_pool(common.memory())
            || prepared.now != step.time()
        {
            return Err(invalid());
        }
        let PreparedFactoryScheduleActionV1 {
            selected,
            outcome,
            sources,
            event_fee,
            phase,
            ..
        } = prepared;
        let action = outcome
            .override_action_for_selected_v1(selected, step.transaction)?
            .unwrap_or(&selected.envelope().action);
        if !is_action(action) {
            return Err(invalid());
        }
        let now = step.time();
        let mut fees = current::SourceFeesV1::new(work.max_source_records, common)?;
        let evaluated = {
            let mut context = WorldFieldOverlayContextV1 {
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
            let mut source = current::CurrentFactoryScheduleFactsV1 {
                sources,
                context: &mut context,
                read: parent,
                work,
                common,
                now,
                live,
                fees: &mut fees,
            };
            match action{
    crate::Action::BuildFactory{builder_agent_id,site_id,spec}=>crate::world::event_processing::factory_schedule_fixed::evaluate_build_factory_v1(&mut source,selected.envelope().id,builder_agent_id,site_id,spec,crate::world::event_processing::factory_schedule_fixed::BuildMaterialCheckV1::Required),
    crate::Action::ScheduleRecipe{requester_agent_id,factory_id,recipe_id,plan,logistics_route_ids,logistics_path_ids}=>crate::world::event_processing::factory_schedule_fixed::evaluate_schedule_recipe_v1(&mut source,selected.envelope().id,requester_agent_id,factory_id,recipe_id,plan,logistics_route_ids,logistics_path_ids),
    _=>Err(current::EvaluationErrorV1::Body(invalid())),
   }
        };
        let mut body = match evaluated {
            Ok(body) => body,
            Err(error) => return Err(evaluation_error(error, fees)),
        };
        if let crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected {
            reason, ..
        }) = body
        {
            let event = step.append_selected_rejection_owned(
                materials,
                live,
                selected,
                reason,
                max_event_bytes,
                audit,
                &mut *persist,
            )?;
            return Ok(CompletedFactoryScheduleActionV1::Rejected {
                accepted: accepted.into_audit(),
                event,
                core: common.work().identity(),
                memory: common.memory().clone(),
                _phase: phase,
                _source_fees: fees,
            });
        }
        // World clears correlation on any actual Rule override, even when
        // the modified action still happens to schedule the same recipe.
        let retained_origin = if outcome
            .override_action_for_selected_v1(selected, step.transaction)?
            .is_none()
        {
            selected.envelope().committed_recipe_origin.as_ref()
        } else {
            None
        };
        attach_recipe_origin_with_work_v1(&mut body, retained_origin, &mut |origin| {
            let mut bytes = size_of::<crate::CommittedRecipeOrigin>();
            for value in origin::origin_texts_v1(origin) {
                charge(common, value.len())?;
                bytes = bytes.checked_add(value.len()).ok_or_else(limited)?;
            }
            fees.hold(bytes, common)
        })?;
        let build = record::build_job_from_event_v1(&body, common)?;
        let recipe = record::recipe_job_from_event_v1(&body, common)?;
        if build.is_some() == recipe.is_some() {
            return Err(invalid());
        }
        // Retain the real selected path owners and allocation plan before any debit.
        // The unique allocator executes the original sorted/duplicate/error order.
        let paths = if let Some(job) = &recipe {
            let evaluated = {
                let mut context = WorldFieldOverlayContextV1 {
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
                let mut source = current::CurrentFactoryScheduleFactsV1 {
                    sources,
                    context: &mut context,
                    read: parent,
                    work,
                    common,
                    now,
                    live,
                    fees: &mut fees,
                };
                source.path_plan(
                    &job.consume_ledger,
                    &job.logistics_route_ids,
                    &job.logistics_path_ids,
                    &job.consume,
                )
            };
            match evaluated {
                Ok(Ok(paths)) => Some(paths),
                Ok(Err(_)) => return Err(invalid()),
                Err(error) => return Err(evaluation_error(error, fees)),
            }
        } else {
            None
        };
        crate::world::incremental_step::factory_schedule_write_io::paid(
            step,
            read,
            parent,
            write,
            materials,
            work,
            common,
            live,
            |step, read, _parent, write, materials, work| {
                if let Some(job) = &build {
                    commit::apply_build_v1(job, sources, step, read, write, materials, work, live)?;
                }
                if let Some(job) = &recipe {
                    let crate::WorldEventBody::Domain(crate::DomainEvent::RecipeStarted {
                        market_quotes,
                        ..
                    }) = &body
                    else {
                        return Err(invalid());
                    };
                    commit::apply_recipe_v1(
                        job,
                        market_quotes,
                        paths.as_ref().ok_or_else(invalid)?,
                        sources,
                        step,
                        read,
                        write,
                        materials,
                        work,
                        live,
                    )?;
                }
                Ok(())
            },
        )?;
        // Original counted publication happens only after every related write has
        // returned and the exclusive state lease has reconciled into this Common.
        let sequence =
            step.read_allocator(crate::world::incremental_allocators::RuntimeAllocatorV1::Event)?;
        let (id, _, _) = crate::world::World::preview_next_event_id(sequence.next, sequence.era);
        let event = crate::WorldEvent {
            id,
            time: now,
            caused_by: Some(crate::CausedBy::Action(selected.envelope().id)),
            body,
        };
        let cap = PreparedFactoryScheduleEventV1 {
            event: event_fee.retain(event),
            accepted: accepted.into_audit(),
            guard: step.transaction.guard(),
            core: common.work().identity(),
            memory: common.memory().clone(),
            _phase: phase,
            _source_fees: fees,
        };
        step.append_prepared_factory_schedule_event_v1(
            &cap,
            materials,
            common,
            live,
            max_event_bytes,
            persist,
        )?;
        Ok(CompletedFactoryScheduleActionV1::Applied(cap))
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = step
            .transaction
            .contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}

#[path = "incremental_normal_action_factory_schedule_origin.rs"]
mod origin;
pub(in crate::world) use origin::attach_recipe_origin_with_work_v1;

pub(in crate::world) use current::EvaluationErrorV1 as FactoryScheduleReadErrorV1;
pub(in crate::world) use io::paid as with_factory_schedule_read_io_v1;

#[path = "incremental_normal_industry_due_priority_current.rs"]
mod due_priority_current;
pub(in crate::world) use due_priority_current::{DuePriorityJobV1, current_due_priority_v1};

pub(in crate::world) use commit::activity::{
    set_blocked_v1 as set_blocked_recipe_activity_v1, set_idle_v1 as set_completed_recipe_idle_v1,
};

#[path = "incremental_normal_industry_due_facts.rs"]
mod due_facts;
pub(in crate::world) use due_facts::{
    DueTerminalCurrentErrorV1, check_due_ledger_destination_v1, read_due_product_module_v1,
};
