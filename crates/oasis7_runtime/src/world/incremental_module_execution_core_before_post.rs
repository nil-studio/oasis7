//! Fixed Core execution split at World::step's SafeAlt finalization point.
//! The existing entry delegates Unbound, then calls the same Post/latest tail.
use super::*;
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
    pub(in crate::world) fn execute_scoped_core_before_post_v1<P, ER, CR, PE, PC>(
        &mut self,
        selected: &SelectedPendingActionV1,
        admission: crate::world::incremental_action_admission::CoreActionAdmissionV1<'_, '_, '_>,
        common: Option<&mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>>,
        outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
        materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<
            '_, M, impl FnMut() -> Option<ValueTreeError>>,
        read: &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<
            '_,
            P,
            R,
            ER,
            CR,
        >,
        parent_read: Option<
            &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<
                '_,
                P,
                R,
                PE,
                PC,
            >,
        >,
        accepted_entries: Option<&mut usize>,
        policy_scan: &mut StateScanBudget,
        policy_work: &mut oasis7_world_state::incremental_runtime_revocation::RevocationReadWorkV1,
        audit_work: &mut oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<CompletedCoreActionV1, CoreDriverErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        ER: FnMut() -> Option<ValueTreeError>,
        CR: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        let result: Result<_, CoreDriverErrorV1<O::Error>> = (|| {
            // The Core reducer reads the effective action from the same
            // Selected-bound outcome; Selected itself remains the original.
            let live = live_owner();
            let guard = self.ctx.transaction.guard();
            let prepared = {
                let mut step = IwsStepContext::borrow_counted_at(
                    self.ctx.transaction,
                    self.now,
                    &guard,
                    &live,
                    self.events.values,
                    &mut self.events.mutation,
                    &mut self.events.original_work,
                    self.events.world_events,
                )
                .map_err(RootModuleExecutionErrorV1::Body)?;
                crate::world::incremental_action_execution_core::prepare_scoped_selected_core_action_v1(
                    &mut step,
                    selected,
                    admission,
                    common,
                    outcome,
                    self.families.agents,
                    read,
                    policy_scan,
                    policy_work,
                    &guard,
                    &live,
                )
                .map_err(RootModuleExecutionErrorV1::Body)?
            };
            let settled = settle_prepared_core_action_v1(
                prepared,
                selected,
                self.ctx,
                self.members,
                &live_owner(),
            )
            .map_err(CoreDriverErrorV1::Settlement)?;
            if crate::world::World::should_emit_action_accepted(&selected.envelope().action) {
                let parent_read = parent_read.ok_or_else(|| {
                    RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                        "selected Core Accepted parent read missing",
                    ))
                })?;
                let pending_entries = accepted_entries.ok_or_else(|| {
                    RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                        "selected Core Accepted work missing",
                    ))
                })?;
                self.append_selected_accepted_disposition_v1(
                    selected,
                    materials,
                    self.families.agents,
                    read,
                    parent_read,
                    pending_entries,
                    audit_work,
                    live_owner,
                )?;
            }
            let live = live_owner();
            let guard = self.ctx.transaction.guard();
            let completed = {
                let mut step = IwsStepContext::borrow_counted_at(
                    self.ctx.transaction,
                    self.now,
                    &guard,
                    &live,
                    self.events.values,
                    &mut self.events.mutation,
                    &mut self.events.original_work,
                    self.events.world_events,
                )
                .map_err(RootModuleExecutionErrorV1::Body)?;
                apply_settled_core_action_v1(
                    &mut step,
                    materials,
                    selected,
                    settled,
                    self.families.agents,
                    read,
                    audit_work,
                    self.events.max_event_bytes,
                    &guard,
                    &live,
                    &mut self.events.persist_original,
                )
                .map_err(RootModuleExecutionErrorV1::Body)?
            };
            let event = PersistedWorldEventV1::from_core_action(&completed);
            // Only the actual resource settlement capability decides whether
            // World::step's pre-PostAction Event branch was taken.
            if completed.event_before_post_action() {
                self.route_persisted_event_v1(&event, None, directory, enumeration, live_owner)?;
            }
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
    /// Run only after the actual SafeAlt finalizer (if any). The original Core
    /// result remains PostAction's input; latest routing reads actual Current.
    pub(in crate::world) fn finish_core_post_action_v1(
        &mut self,
        selected: &SelectedPendingActionV1,
        outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
        completed: &CompletedCoreActionV1,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        tail_work: &mut crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<(), CoreDriverErrorV1<O::Error>> {
        let result = (|| {
            completed.validate(self.ctx.transaction).map_err(|error| {
                RootModuleExecutionErrorV1::Body(BodyMutationError::Execution(error))
            })?;
            let event = PersistedWorldEventV1::from_core_action(completed);
            self.route_post_action_after_rules_v1(
                selected,
                outcome,
                Some(&event),
                directory,
                enumeration,
                live_owner,
            )?;
            self.route_latest_world_event_v1(directory, enumeration, tail_work, live_owner)?;
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
