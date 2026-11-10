//! ContractAccept settlement, selective Accepted and post routing in the actual Root context.
use super::*;
use crate::world::{
    incremental_action_admission::UnboundActionAdmissionV1,
    incremental_action_contract_accept::{
        CompletedContractAcceptV1, ContractAcceptRejectionReservationV1, ContractAcceptSourcesV1,
        ContractAcceptWorkV1, PreparedContractAcceptActionV1, commit_prepared_contract_accept_v1,
        prepare_selected_contract_accept_v1,
    },
    incremental_action_execution::SelectedPendingActionV1,
    incremental_module_execution_event::PersistedWorldEventV1,
    incremental_module_invocation::RootRulePhaseOutcomeV1,
    incremental_step::IwsStepContext,
};

use super::ordinary_settlement::{OrdinaryDriverErrorV1, settle_ordinary_rules_v1};

enum ContractAcceptAfterRulesV1<'s, 'r> {
    Rejected {
        reason: crate::RejectReason,
        prepaid: ContractAcceptRejectionReservationV1,
    },
    Prepared(PreparedContractAcceptActionV1<'s, 'r>),
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
    pub(in crate::world) fn execute_contract_accept_after_rules_v1<P, ER, CR, PE, PC>(
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
        contract_accept_work: &mut ContractAcceptWorkV1,
        sources: &ContractAcceptSourcesV1<'_>,
        accepted_entries: Option<&mut usize>,
        audit_work: &mut oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        tail_work: &mut crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<CompletedContractAcceptV1, OrdinaryDriverErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        ER: FnMut() -> Option<ValueTreeError>,
        CR: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        let result: Result<_, OrdinaryDriverErrorV1<O::Error>> = (|| {
            let contract_accept_family = |action: &crate::Action| {
                matches!(action, crate::Action::AcceptEconomicContract { .. })
            };
            let live = live_owner();
            if self.ctx.values.class != MemoryClass::Ordinary
                || !self
                    .events
                    .mutation
                    .memory
                    .same_pool(self.ctx.values.memory)
                || !self.members.memory.same_pool(self.ctx.values.memory)
            {
                return Err(OrdinaryDriverErrorV1::Module(
                    RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                        "contract_accept driver context pool/class",
                    )),
                ));
            }
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
            if !contract_accept_family(effective) || !outcome.uses_budget(self.ctx.values.memory) {
                return Err(OrdinaryDriverErrorV1::Module(
                    RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                        "contract_accept driver action/rule/pool binding",
                    )),
                ));
            }
            if (self.events.mutation.cancelled)() {
                return Err(RootModuleExecutionErrorV1::Value(ValueTreeError::Cancelled).into());
            }
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
            let guard = self.ctx.transaction.guard();
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
                ContractAcceptAfterRulesV1::Rejected {
                    reason,
                    prepaid: CompletedContractAcceptV1::reserve_rejection(
                        self.events.mutation.memory,
                    )
                    .map_err(RootModuleExecutionErrorV1::Body)?,
                }
            } else {
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
                ContractAcceptAfterRulesV1::Prepared(
                    prepare_selected_contract_accept_v1(
                        &mut step,
                        selected,
                        outcome,
                        sources,
                        self.families.roots,
                        read,
                        parent_read,
                        contract_accept_work,
                        &guard,
                        &live,
                    )
                    .map_err(RootModuleExecutionErrorV1::Body)?,
                )
            };
            let accepted = self.append_selected_accepted_disposition_v1(
                selected,
                materials,
                sources.agents,
                read,
                parent_read,
                accepted_entries.unwrap_or(&mut contract_accept_work.pending_entries_remaining),
                audit_work,
                live_owner,
            )?;
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
                match prepared {
                    ContractAcceptAfterRulesV1::Rejected { reason, prepaid } => {
                        let rejected = step
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
                        CompletedContractAcceptV1::from_rejection(
                            accepted,
                            rejected,
                            selected,
                            step.transaction,
                            step.mutation.memory,
                            prepaid,
                        )
                        .map_err(RootModuleExecutionErrorV1::Body)?
                    }
                    ContractAcceptAfterRulesV1::Prepared(prepared) => {
                        commit_prepared_contract_accept_v1(
                            prepared,
                            accepted,
                            &mut step,
                            materials,
                            sources,
                            read,
                            parent_read,
                            contract_accept_work,
                            audit_work,
                            self.events.max_event_bytes,
                            &live,
                            &mut self.events.persist_original,
                        )
                        .map_err(RootModuleExecutionErrorV1::Body)?
                    }
                }
            };
            let event = PersistedWorldEventV1::from_contract_accept(&completed);
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
