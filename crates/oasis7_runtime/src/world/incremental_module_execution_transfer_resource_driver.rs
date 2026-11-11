//! ResourceTransfer settlement and post routing in the actual Root context.
use super::*;
use crate::world::{
    incremental_action_admission::UnboundActionAdmissionV1,
    incremental_action_data::transfer_resource::{
        CompletedTransferResourceV1, TransferResourceWorkV1, execute_selected_transfer_resource_v1,
    },
    incremental_action_execution::SelectedPendingActionV1,
    incremental_module_execution_event::PersistedWorldEventV1,
    incremental_module_invocation::RootRulePhaseOutcomeV1,
    incremental_step::IwsStepContext,
};

use super::ordinary_settlement::{OrdinaryDriverErrorV1, settle_ordinary_rules_v1};

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
    /// Consume the genuine pending source and Rule outcome. Public command
    /// ingress and internal Emit origin admission stay with their existing
    /// owners. A native TransferResource still needs its real Rule conversion;
    /// the business owner retains native rejection and Emit ledger semantics.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn execute_transfer_resource_after_rules_v1<P, ER, CR, PE, PC>(
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
        transfer_work: &mut TransferResourceWorkV1,
        accepted_entries: Option<&mut usize>,
        audit_work: &mut oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        tail_work: &mut crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<CompletedTransferResourceV1, OrdinaryDriverErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        ER: FnMut() -> Option<ValueTreeError>,
        CR: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        let result: Result<_, OrdinaryDriverErrorV1<O::Error>> = (|| {
            let transfer_family = |action: &crate::Action| {
                matches!(
                    action,
                    crate::Action::TransferResource { .. }
                        | crate::Action::EmitResourceTransfer { .. }
                )
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
                        "TransferResource driver context pool/class",
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
            if !transfer_family(effective) || !outcome.uses_budget(self.ctx.values.memory) {
                return Err(OrdinaryDriverErrorV1::Module(
                    RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                        "TransferResource driver action/rule/pool binding",
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
            if crate::world::World::should_emit_action_accepted(&selected.envelope().action) {
                self.append_selected_accepted_disposition_v1(
                    selected,
                    materials,
                    self.families.agents,
                    read,
                    parent_read,
                    accepted_entries.unwrap_or(&mut transfer_work.pending_entries_remaining),
                    audit_work,
                    live_owner,
                )?;
            }
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
                if let Some(reason) = rejection {
                    CompletedTransferResourceV1::from_rejection(
                        step.append_selected_rejection_owned(
                            materials,
                            &live,
                            selected,
                            reason,
                            self.events.max_event_bytes,
                            audit_work,
                            &mut self.events.persist_original,
                        )
                        .map_err(RootModuleExecutionErrorV1::Body)?,
                        selected,
                        step.transaction,
                        step.mutation.memory,
                    )
                    .map_err(RootModuleExecutionErrorV1::Body)?
                } else {
                    execute_selected_transfer_resource_v1(
                        &mut step,
                        materials,
                        selected,
                        outcome,
                        self.families.agents,
                        self.families.roots,
                        read,
                        parent_read,
                        transfer_work,
                        audit_work,
                        self.events.max_event_bytes,
                        &guard,
                        &live,
                        &mut self.events.persist_original,
                    )
                    .map_err(RootModuleExecutionErrorV1::Body)?
                }
            };
            let event = PersistedWorldEventV1::from_transfer_resource(&completed);
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
