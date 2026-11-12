//! Fixed real after-Rule owners for Observation and body-interface expansion.
use super::ordinary_settlement::{OrdinaryDriverErrorV1, settle_ordinary_rules_v1};
use super::*;
use crate::world::{
    incremental_action_admission::UnboundActionAdmissionV1,
    incremental_action_execution::{SelectedActionAuditV1, SelectedPendingActionV1},
    incremental_module_execution_event::PersistedWorldEventV1,
    incremental_module_invocation::RootRulePhaseOutcomeV1,
    incremental_normal_action_body_interface::{
        CompletedBodyInterfaceV1, PreparedBodyInterfaceActionV1, commit_body_interface_v1,
        prepare_body_interface_v1,
    },
    incremental_normal_action_core_extension::{
        CompletedObservationActionV1, PreparedObservationActionV1, commit_observation_action_v1,
        prepare_observation_action_v1,
    },
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::{
    FormalBoundedContextV1, FormalBoundedWorkV1,
};
use oasis7_formal_protocol::incremental_execution::ExecutionTransaction;
pub(in crate::world) struct NormalCoreExtensionWorkV1<'a> {
    pub(in crate::world) common: &'a FormalBoundedWorkV1,
    pub(in crate::world) pending_entries_remaining: usize,
    pub(in crate::world) max_body_text_bytes: usize,
}
pub(in crate::world) struct NormalCoreExtensionWorkViewV1<'a> {
    common: &'a FormalBoundedWorkV1,
    pending_entries_remaining: &'a mut usize,
    max_body_text_bytes: usize,
}
impl NormalCoreExtensionWorkViewV1<'_> {
    pub(in crate::world) fn reborrow(&mut self) -> NormalCoreExtensionWorkViewV1<'_> {
        NormalCoreExtensionWorkViewV1 {
            common: self.common,
            pending_entries_remaining: &mut *self.pending_entries_remaining,
            max_body_text_bytes: self.max_body_text_bytes,
        }
    }
}
impl NormalCoreExtensionWorkV1<'_> {
    pub(in crate::world) fn view(&mut self) -> NormalCoreExtensionWorkViewV1<'_> {
        NormalCoreExtensionWorkViewV1 {
            common: self.common,
            pending_entries_remaining: &mut self.pending_entries_remaining,
            max_body_text_bytes: self.max_body_text_bytes,
        }
    }
}
enum PreparedExtensionV1<'s, 'r> {
    Rejected(crate::RejectReason),
    Observation(PreparedObservationActionV1<'s, 'r>),
    Body(PreparedBodyInterfaceActionV1<'s, 'r>),
}
enum CompletedExtensionInnerV1 {
    Rejected {
        accepted: Option<SelectedActionAuditV1>,
        event: SelectedActionAuditV1,
    },
    Observation(CompletedObservationActionV1),
    Body(CompletedBodyInterfaceV1),
}
pub(in crate::world) struct CompletedNormalExtensionV1 {
    inner: CompletedExtensionInnerV1,
    _layout: ReservedValue<()>,
}
impl CompletedNormalExtensionV1 {
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        match &self.inner {
            CompletedExtensionInnerV1::Rejected { event, .. } => event.event(),
            CompletedExtensionInnerV1::Observation(value) => value.event(),
            CompletedExtensionInnerV1::Body(value) => value.event(),
        }
    }
    pub(in crate::world) fn validate<N: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, N>,
    ) -> Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> {
        match &self.inner {
            CompletedExtensionInnerV1::Rejected { accepted, event } => {
                if let Some(accepted) = accepted {
                    accepted.validate(tx)?;
                }
                event.validate(tx)
            }
            CompletedExtensionInnerV1::Observation(value) => value.validate(tx),
            CompletedExtensionInnerV1::Body(value) => value.validate(tx),
        }
    }

    fn persisted(&self) -> PersistedWorldEventV1<'_> {
        match &self.inner {
            CompletedExtensionInnerV1::Rejected { event, .. } => {
                PersistedWorldEventV1::from_action_audit(event)
            }
            CompletedExtensionInnerV1::Observation(value) => {
                PersistedWorldEventV1::from_observation(value)
            }
            CompletedExtensionInnerV1::Body(value) => {
                PersistedWorldEventV1::from_body_interface(value)
            }
        }
    }
}
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
    /// Genuine pre/post-Rule admission is supplied by the selected lane. The
    /// private owner preflights business before the genuine selective Accepted.
    /// The actual result follows that audit, including rules/fee rejections.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn execute_normal_extension_after_rules_v1<P, ER, CR, PE, PC>(
        &mut self,
        selected: &SelectedPendingActionV1,
        admission: &UnboundActionAdmissionV1<'_>,
        outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
        materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<'_, M, impl FnMut() -> Option<ValueTreeError>>,
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
        work: &mut NormalCoreExtensionWorkViewV1<'_>,
        accepted_entries: Option<&mut usize>,
        audit_work: &mut oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        tail_work: &mut crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<CompletedNormalExtensionV1, OrdinaryDriverErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        ER: FnMut() -> Option<ValueTreeError>,
        CR: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        let result: Result<_, OrdinaryDriverErrorV1<O::Error>> = (|| {
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
            if !matches!(
                effective,
                crate::Action::EmitObservation { .. } | crate::Action::ExpandBodyInterface { .. }
            ) || !outcome.uses_budget(self.ctx.values.memory)
                || !self
                    .events
                    .mutation
                    .memory
                    .same_pool(self.ctx.values.memory)
                || self.ctx.values.class != MemoryClass::Ordinary
            {
                return Err(RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                    "normal extension selected source",
                ))
                .into());
            }
            let layout = self
                .ctx
                .values
                .memory
                .reserve(
                    std::mem::size_of::<CompletedNormalExtensionV1>(),
                    MemoryClass::Ordinary,
                )
                .map_err(ValueTreeError::Memory)
                .map_err(RootModuleExecutionErrorV1::Value)?
                .retain(());
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
            let prepared = if let Some(reason) = rejection {
                PreparedExtensionV1::Rejected(reason)
            } else {
                let mut common = FormalBoundedContextV1::new(
                    self.ctx.values.memory,
                    work.common,
                    &mut self.events.mutation.cancelled,
                );
                if matches!(effective, crate::Action::EmitObservation { .. }) {
                    PreparedExtensionV1::Observation(
                        prepare_observation_action_v1(
                            selected,
                            outcome,
                            self.ctx.transaction,
                            self.now,
                            &mut common,
                            &live,
                        )
                        .map_err(RootModuleExecutionErrorV1::Body)?,
                    )
                } else {
                    let mut overlay =
                        oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1 {
                            transaction: self.ctx.transaction,
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
                    PreparedExtensionV1::Body(
                        prepare_body_interface_v1(
                            selected,
                            outcome,
                            self.families.agents,
                            &mut overlay,
                            &mut common,
                            self.now,
                            work.max_body_text_bytes,
                            &live,
                        )
                        .map_err(RootModuleExecutionErrorV1::Body)?,
                    )
                }
            };
            // Original selectivity and actor stay attached to Selected even if
            // a Rule chose this effective owner from a different family.
            let accepted = self.append_selected_accepted_disposition_v1(
                selected,
                materials,
                self.families.agents,
                read,
                parent_read,
                accepted_entries.unwrap_or(&mut *work.pending_entries_remaining),
                audit_work,
                live_owner,
            )?;
            let live = live_owner();
            let guard = self.ctx.transaction.guard();
            let inner = {
                let mut common = FormalBoundedContextV1::new(
                    self.ctx.values.memory,
                    work.common,
                    &mut parent_read.cancelled,
                );
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
                accepted
                    .validate(selected, step.transaction, step.mutation.memory)
                    .map_err(RootModuleExecutionErrorV1::Body)?;
                match prepared {
                    PreparedExtensionV1::Rejected(reason) => CompletedExtensionInnerV1::Rejected {
                        accepted: accepted.into_audit(),
                        event: step
                            .append_selected_rejection_owned(
                                materials,
                                &live,
                                selected,
                                reason,
                                self.events.max_event_bytes,
                                audit_work,
                                &mut self.events.persist_original,
                            )
                            .map_err(RootModuleExecutionErrorV1::Body)?,
                    },
                    PreparedExtensionV1::Observation(value) => {
                        CompletedExtensionInnerV1::Observation(
                            commit_observation_action_v1(
                                value,
                                accepted,
                                &mut step,
                                materials,
                                &mut common,
                                self.events.max_event_bytes,
                                &live,
                                &mut self.events.persist_original,
                            )
                            .map_err(RootModuleExecutionErrorV1::Body)?,
                        )
                    }
                    PreparedExtensionV1::Body(value) => CompletedExtensionInnerV1::Body(
                        commit_body_interface_v1(
                            value,
                            accepted,
                            &mut step,
                            read,
                            &mut common,
                            materials,
                            work.max_body_text_bytes,
                            self.events.max_event_bytes,
                            audit_work,
                            &live,
                            &mut self.events.persist_original,
                        )
                        .map_err(RootModuleExecutionErrorV1::Body)?,
                    ),
                }
            };
            let completed = CompletedNormalExtensionV1 {
                inner,
                _layout: layout,
            };
            let event = completed.persisted();
            if applied {
                self.route_persisted_event_v1(&event, None, directory, enumeration, live_owner)?;
            }
            self.route_post_action_after_rules_v1(
                selected,
                outcome,
                Some(&event),
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
