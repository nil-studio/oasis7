//! Completion graph derived only from an actual committed release install.
//! The sole legacy ordering kernel is reused; there is no caller-authored event.
use super::instance_tail_commit::CommittedModuleInstallStateV1;
use super::*;
use crate::world::module_release_publication::ModuleReleaseCompletion;

pub(super) struct PaidCurrentReleaseCompletionV1<'a> {
    source: &'a CommittedModuleInstallStateV1,
    bodies: Vec<crate::DomainEvent>,
    core: WorkIdentityV1,
    now: u64,
    fee: MemoryReservation,
}
impl PaidCurrentReleaseCompletionV1<'_> {
    /// Final request writers are bound to this exact selected release and its
    /// original Rule outcome, in addition to the committed installation.
    pub(super) fn validate_selection<M: MerkleNodeReader>(
        &self, selected:&SelectedPendingActionV1, outcome:&ReservedValue<RootRulePhaseOutcomeV1>,
        tx:&ExecutionTransaction<'_,M>, memory:&oasis7_proto::memory_budget::MemoryBudget,
        core:WorkIdentityV1, now:u64, live:&ExecutionFence,
    )->Result<(),BodyMutationError>{
        self.validate(tx,memory,core,now,live)?;
        self.source.release_source().ok_or_else(invalid)?.validate_provenance(selected,outcome,tx,memory,core,now,live)
    }
    pub(super) fn bodies(&self) -> &[crate::DomainEvent] {
        &self.bodies
    }
    pub(super) fn final_event(&self) -> Result<&crate::DomainEvent, BodyMutationError> {
        self.bodies
            .last()
            .filter(|body| matches!(body, crate::DomainEvent::ModuleReleaseApplied { .. }))
            .ok_or_else(invalid)
    }
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        core: WorkIdentityV1,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.source.validate(tx, memory, now, core, live)?;
        if self.core != core || self.now != now || !self.fee.uses_budget(memory) {
            return Err(invalid());
        }
        self.final_event()?;
        Ok(())
    }
}
pub(super) fn prepare_current_release_completion_v1<'a, M: MerkleNodeReader>(
    source: &'a CommittedModuleInstallStateV1,
    tx: &ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    now: u64,
    live: &ExecutionFence,
) -> Result<PaidCurrentReleaseCompletionV1<'a>, BodyMutationError> {
    source.validate(tx, common.memory(), now, common.work().identity(), live)?;
    let release = source.release_source().ok_or_else(invalid)?;
    let request = release.request();
    let measured = match super::governance_json::module_json_value_paid_v1(
        &(
            &request.profile_changes,
            source.event(),
            release.operator_id(),
        ),
        common,
    ) {
        Ok(value) => value,
        Err(super::governance_json::ModuleGovernanceJsonErrorV1::Body(error)) => return Err(error),
        Err(super::governance_json::ModuleGovernanceJsonErrorV1::Json { .. }) => {
            return Err(invalid());
        }
    };
    let count = request
        .profile_changes
        .product_profiles
        .len()
        .checked_add(request.profile_changes.recipe_profiles.len())
        .and_then(|n| n.checked_add(request.profile_changes.factory_profiles.len()))
        .and_then(|n| n.checked_add(1))
        .ok_or_else(invalid)?;
    let graph = measured
        .fee
        .bytes()
        .checked_mul(8)
        .and_then(|n| n.checked_add(count.checked_mul(std::mem::size_of::<crate::DomainEvent>())?))
        .and_then(|n| n.checked_add(32768))
        .ok_or_else(invalid)?;
    // Stable sorting compares at most O(n log n) identifiers. Charge the entire
    // graph at every comparison level before either sorting or copying it.
    let levels = usize::BITS as usize - count.leading_zeros() as usize + 1;
    pay(common, graph.checked_mul(levels).ok_or_else(invalid)?)?;
    let fee = common
        .memory()
        .reserve(graph, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let bodies = ModuleReleaseCompletion {
        request_id: release.outer_request_id(),
        operator_agent_id: release.operator_id().to_owned(),
        profile_changes: request.profile_changes.clone(),
    }
    .events(source.event());
    Ok(PaidCurrentReleaseCompletionV1 {
        source,
        bodies,
        core: common.work().identity(),
        now,
        fee,
    })
}

pub(in crate::world) struct PreparedCurrentReleaseProfileEventV1<'a, 'source> {
    source: &'a PaidCurrentReleaseCompletionV1<'source>,
    index: usize,
    event: ReservedValue<crate::WorldEvent>,
}
impl PreparedCurrentReleaseProfileEventV1<'_, '_> {
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        &self.event
    }
    pub(in crate::world) fn action_id(&self) -> u64 {
        self.source.source.action_id()
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        core: WorkIdentityV1,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.source.validate(tx, memory, core, now, live)?;
        let body = self.source.bodies.get(self.index).ok_or_else(invalid)?;
        if !matches!(
            body,
            crate::DomainEvent::ProductProfileGoverned { .. }
                | crate::DomainEvent::RecipeProfileGoverned { .. }
                | crate::DomainEvent::FactoryProfileGoverned { .. }
        ) || !self.event.uses_budget(memory)
            || self.event.time != now
            || self.event.caused_by != Some(crate::CausedBy::Action(self.action_id()))
        {
            return Err(invalid());
        }
        // This immutable event is cloned below only from the exact paid slot.
        if !matches!(&self.event.body,crate::WorldEventBody::Domain(actual) if actual==body) {
            return Err(invalid());
        }
        Ok(())
    }
}

/// Publish only the sorted profile prefix. The caller commits the final real
/// request/mapping transition afterwards and routes only that final event.
#[allow(clippy::too_many_arguments)]
pub(super) fn commit_current_release_profiles_v1<M, P, V, W, D, E, C, PE, PC, CW, ME, F>(
    completion: &PaidCurrentReleaseCompletionV1<'_>,
    sources: &ModuleLifecycleSourcesV1<'_>,
    step: &mut crate::world::incremental_step::IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<
        '_,
        P,
        V,
        PE,
        PC,
    >,
    write: &mut oasis7_world_state::incremental_field_member::MemberRecordWriteContext<'_, W, CW>,
    materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<
        '_,
        M,
        ME,
    >,
    module: &mut ModuleLifecycleWorkV1<'_>,
    factory: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    max_event_bytes: usize,
    persist: &mut F,
    live: &ExecutionFence,
) -> Result<(), BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: oasis7_formal_protocol::incremental_value::ValueNodeReader,
    W: oasis7_formal_protocol::incremental_value::ValueNodeReader
        + oasis7_formal_protocol::incremental_value::ValueNodeWriter
        + oasis7_formal_protocol::incremental_state::StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
{
    use oasis7_formal_protocol::incremental_value::ValueReadContext;
    use oasis7_world_state::{
        incremental_field_overlay::WorldFieldOverlayContextV1,
        incremental_field_read::WorldFieldReadContextV1,
    };
    let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        completion.validate(
            step.transaction,
            common.memory(),
            common.work().identity(),
            step.time(),
            live,
        )?;
        for (index, body) in completion
            .bodies
            .iter()
            .enumerate()
            .take(completion.bodies.len().saturating_sub(1))
        {
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
                |step, read, parent, write, _materials, factory, _module, work| {
                    completion.validate(
                        step.transaction,
                        work.memory(),
                        work.identity(),
                        step.time(),
                        live,
                    )?;
                    let mut native = WorldFieldReadContextV1 {
                        root: sources.families.roots.state_root(
                            oasis7_formal_protocol::incremental_state::StateNamespaceV1::Business,
                        ),
                        state: parent.state,
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
                    macro_rules! publish {
                        ($profile:expr,$id:expr,$reader:ident,$writer:ident) => {{
                            let guard = step.transaction.guard();
                            let before = oasis7_world_state::incremental_world_native::$reader(
                                $id,
                                sources.factory.lifecycle.native,
                                sources.families.roots,
                                step.transaction,
                                &mut native,
                                &mut factory.lifecycle.native_scan,
                                &mut factory.lifecycle.native_read,
                                &guard,
                                live,
                            )
                            .map_err(overlay_error)?;
                            oasis7_world_state::incremental_world_native::$writer(
                                &before,
                                $id,
                                $profile,
                                step.transaction,
                                &mut native,
                                write,
                                &mut factory.lifecycle.records_remaining,
                                &mut factory.lifecycle.native_write,
                                live,
                            )
                            .map_err(overlay_error)?;
                        }};
                    }
                    match body {
                        crate::DomainEvent::ProductProfileGoverned { profile, .. } => publish!(
                            profile,
                            &profile.product_id,
                            read_current_governed_product_profile_v1,
                            write_current_governed_product_profile_v1
                        ),
                        crate::DomainEvent::RecipeProfileGoverned { profile, .. } => publish!(
                            profile,
                            &profile.recipe_id,
                            read_current_governed_recipe_profile_v1,
                            write_current_governed_recipe_profile_v1
                        ),
                        crate::DomainEvent::FactoryProfileGoverned { profile, .. } => {
                            let guard = step.transaction.guard();
                            let before=oasis7_world_state::incremental_world_native::read_current_governed_factory_profile_v1(&profile.factory_id,sources.factory.lifecycle.native,sources.families.roots,step.transaction,&mut native,&mut factory.lifecycle.native_scan,&mut factory.lifecycle.native_read,&guard,live).map_err(overlay_error)?;
                            // Original release semantics retain an existing factory profile after
                            // compatibility validation; only a missing profile is installed.
                            if before.value().is_none() {
                                oasis7_world_state::incremental_world_native::write_current_governed_factory_profile_v1(&before,&profile.factory_id,profile,step.transaction,&mut native,write,&mut factory.lifecycle.records_remaining,&mut factory.lifecycle.native_write,live).map_err(overlay_error)?;
                            }
                        }
                        _ => return Err(invalid()),
                    }
                    drop(native);
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
                    let operator = completion
                        .source
                        .release_source()
                        .ok_or_else(invalid)?
                        .operator_id();
                    // Re-read after the installation fee: the operator may be that installer.
                    let agent = super::current::read_market_agent_v1(
                        operator, sources, &mut ctx, parent, factory, live,
                    )?
                    .ok_or_else(invalid)?;
                    super::commit::write_market_agent_v1(
                        &agent,
                        &agent.stock,
                        completion.now,
                        sources,
                        &mut ctx,
                        write,
                        live,
                    )?;
                    Ok(())
                },
            )?;
            let heap = completion
                .fee
                .bytes()
                .checked_add(std::mem::size_of::<PreparedCurrentReleaseProfileEventV1>())
                .ok_or_else(invalid)?;
            pay(common, heap)?;
            let fee = common
                .memory()
                .reserve(heap, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
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
            let event = PreparedCurrentReleaseProfileEventV1 {
                source: completion,
                index,
                event: fee.retain(crate::WorldEvent {
                    id,
                    time: step.time(),
                    caused_by: Some(crate::CausedBy::Action(completion.source.action_id())),
                    body: crate::WorldEventBody::Domain(body.clone()),
                }),
            };
            let cap=crate::world::incremental_normal_system_event::PreparedNormalSystemEventV1::from_release_profile(&event,step.transaction,step.time(),common,live)?;
            step.append_prepared_normal_system_event_v1(
                &cap,
                materials,
                common,
                live,
                max_event_bytes,
                &mut *persist,
            )?;
        }
        Ok(())
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
