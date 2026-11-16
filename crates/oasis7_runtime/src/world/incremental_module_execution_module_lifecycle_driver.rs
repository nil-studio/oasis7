//! Genuine Rule settlement/selective Accepted around the fixed market consumer.
use super::ordinary_settlement::{OrdinaryDriverErrorV1, settle_ordinary_rules_v1};
use super::*;
use crate::world::incremental_module_execution_event::PersistedWorldEventV1;
use crate::world::{
    incremental_action_admission::UnboundActionAdmissionV1,
    incremental_action_execution::SelectedPendingActionV1,
    incremental_module_invocation::RootRulePhaseOutcomeV1,
    incremental_normal_action_factory_schedule::FactoryScheduleWorkV1,
    incremental_normal_action_module_lifecycle::{
        CompletedModuleLifecycleActionV1, ModuleLifecycleSourcesV1, ModuleLifecycleWorkV1,
        execute_market_after_accepted_v1,
    },
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::{
    FormalBoundedContextV1, FormalBoundedWorkV1,
};
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader
        + ValueNodeWriter
        + oasis7_formal_protocol::incremental_state::StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn execute_module_lifecycle_after_rules_v1<P, ER, CR, PE, PC>(
        &mut self,
        selected: &SelectedPendingActionV1,
        admission: &UnboundActionAdmissionV1<'_>,
        outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
        materials:&mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<'_,M,impl FnMut()->Option<ValueTreeError>>,
        read: &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<
            '_,
            P,
            R,
            ER,
            CR,
        >,
        parent_read: &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<
            '_,
            P,
            R,
            PE,
            PC,
        >,
        work: &mut ModuleLifecycleWorkV1<'_>,
        sources: &ModuleLifecycleSourcesV1<'_>,
        factory: &mut FactoryScheduleWorkV1,
        common_work: &FormalBoundedWorkV1,
        accepted_entries: Option<&mut usize>,
        audit_work: &mut oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        tail_work: &mut crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<CompletedModuleLifecycleActionV1, OrdinaryDriverErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        ER: FnMut() -> Option<ValueTreeError>,
        CR: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        let result = (|| {
            let live = live_owner();
            admission
                .validate(
                    selected,
                    self.ctx.transaction,
                    self.ctx.values.memory,
                    &live,
                )
                .map_err(RootModuleExecutionErrorV1::from)?;
            let effective = outcome
                .override_action_for_selected_v1(selected, self.ctx.transaction)
                .map_err(RootModuleExecutionErrorV1::Body)?
                .unwrap_or(&selected.envelope().action);
            if !(crate::world::incremental_normal_action_module_lifecycle::rules::is_market_action_v1(effective)
                || crate::world::incremental_normal_action_module_lifecycle::is_deployment_action_v1(effective)
                || crate::world::incremental_normal_action_module_lifecycle::is_instance_action_v1(effective) || crate::world::incremental_normal_action_module_lifecycle::is_release_action_v1(effective)) || !outcome.uses_budget(self.ctx.values.memory)
                || !self.members.memory.same_pool(self.ctx.values.memory)
                || !self
                    .events
                    .mutation
                    .memory
                    .same_pool(self.ctx.values.memory)
            {
                return Err(RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                    "module lifecycle driver source",
                ))
                .into());
            }
            let layout = self
                .ctx
                .values
                .memory
                .reserve(
                    std::mem::size_of::<CompletedModuleLifecycleActionV1>(),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)
                .map_err(RootModuleExecutionErrorV1::Value)?;
            let mut settlement = settle_ordinary_rules_v1(
                selected,
                admission,
                outcome,
                self.ctx,
                self.members,
                live_owner,
            )
            .map_err(OrdinaryDriverErrorV1::Settlement)?;
            let applied = settlement.applied();
            let rejection = settlement.take_rejection();
            let live = live_owner();
            settlement
                .validate(
                    selected,
                    outcome,
                    self.ctx.transaction,
                    self.ctx.values.memory,
                    &live,
                )
                .map_err(RootModuleExecutionErrorV1::Body)?;
            let accepted = self.append_selected_accepted_disposition_v1(
                selected,
                materials,
                self.families.agents,
                read,
                parent_read,
                accepted_entries.unwrap_or(&mut *work.pending_entries),
                audit_work,
                live_owner,
            )?;
            let live = live_owner();
            let guard = self.ctx.transaction.guard();
            let completed = {
                let mut common = FormalBoundedContextV1::new(
                    self.ctx.values.memory,
                    common_work,
                    &mut self.ctx.cancelled,
                );
                let mut step = IwsStepContext::borrow_counted_at(
                    self.ctx.transaction,
                    self.now,
                    &guard,
                    &live,
                    self.events.values,
                    &mut self.events.mutation,
                    &mut self.events.original_work,
                    &self.events.world_events,
                )
                .map_err(RootModuleExecutionErrorV1::Body)?;
                accepted
                    .validate(selected, step.transaction, step.mutation.memory)
                    .map_err(RootModuleExecutionErrorV1::Body)?;
                if let Some(reason) = rejection {
                    let event = step
                        .append_selected_rejection_owned(
                            materials,
                            &live,
                            selected,
                            reason,
                            self.events.max_event_bytes,
                            audit_work,
                            &mut self.events.persist_original,
                        )
                        .map_err(RootModuleExecutionErrorV1::Body)?;
                    CompletedModuleLifecycleActionV1::Rejected {
                        accepted: accepted.into_audit(),
                        event,
                        core: common.work().identity(),
                        memory: common.memory().clone(),
                        _layout: layout,
                        _prepare_fees: std::array::from_fn(|_| None),
                    }
                } else {
                    // The business consumer applies the original preflight only after this
                    // actual Accepted fee; its two-event match batch routes below as a batch.
                    if crate::world::incremental_normal_action_module_lifecycle::is_release_apply_action_v1(effective) {
                        crate::world::incremental_normal_action_module_lifecycle::execute_release_apply_after_accepted_v1(
                            selected,outcome,accepted,sources,&mut step,read,parent_read,self.members,materials,work,factory,&mut common,directory,self.events.max_event_bytes,&mut self.events.persist_original,&live,
                        ).map_err(RootModuleExecutionErrorV1::Body)?
                    } else if crate::world::incremental_normal_action_module_lifecycle::is_release_action_v1(effective) {
                        crate::world::incremental_normal_action_module_lifecycle::execute_release_after_accepted_v1(
                            selected,outcome,accepted,sources,&mut step,read,parent_read,self.members,materials,work,factory,&mut common,directory,self.events.max_event_bytes,&mut self.events.persist_original,&live,
                        ).map_err(RootModuleExecutionErrorV1::Body)?
                    } else if crate::world::incremental_normal_action_module_lifecycle::is_instance_action_v1(effective) {
                        crate::world::incremental_normal_action_module_lifecycle::execute_install_after_accepted_v1(
                            selected, outcome, accepted, sources, &mut step, read, parent_read,
                            self.members, materials, work, factory, &mut common, directory,
                            self.events.max_event_bytes, &mut self.events.persist_original, &live,
                        ).map_err(RootModuleExecutionErrorV1::Body)?
                    } else if crate::world::incremental_normal_action_module_lifecycle::is_deployment_action_v1(effective) {
                    crate::world::incremental_normal_action_module_lifecycle::execute_deployment_after_accepted_v1(
                        selected,
                        outcome,
                        accepted,
                        sources,
                        &mut step,
                        read,
                        parent_read,
                        self.members,
                        materials,
                        work,
                        factory,
                        &mut common,
                        &mut *self.originals,
                        directory,
                        self.events.max_event_bytes,
                        audit_work,
                        &mut self.events.persist_original,
                        &live,
                    )
                    ?
                    } else {
                    execute_market_after_accepted_v1(
                        selected,
                        outcome,
                        accepted,
                        sources,
                        &mut step,
                        read,
                        parent_read,
                        self.members,
                        materials,
                        work,
                        factory,
                        &mut common,
                        directory,
                        self.events.max_event_bytes,
                        audit_work,
                        &mut self.events.persist_original,
                        &live,
                    )
                    .map_err(RootModuleExecutionErrorV1::Body)?
                    }
                }
            };
            // Legacy runtime-module actions consume their own domain event stream.
            // They leave PostAction's result unset; only settlement rejection supplies it.
            let event = if applied {
                None
            } else {
                let live = live_owner();
                let mut common = FormalBoundedContextV1::new(
                    self.ctx.values.memory,
                    common_work,
                    &mut self.ctx.cancelled,
                );
                Some(
                    PersistedWorldEventV1::from_module_lifecycle(
                        &completed,
                        self.ctx.transaction,
                        self.now,
                        &mut common,
                        &live,
                    )
                    .map_err(RootModuleExecutionErrorV1::Body)?,
                )
            };
            self.route_post_action_after_rules_v1(
                selected,
                outcome,
                event.as_ref(),
                directory,
                enumeration,
                live_owner,
            )?;
            self.route_latest_world_event_v1(directory, enumeration, tail_work, live_owner)?;
            Ok(completed)
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
