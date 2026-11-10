//! CollectData settlement and post routing in the actual Root context.
use super::*;
use crate::world::{
    incremental_action_admission::UnboundActionAdmissionV1,
    incremental_action_data::{
        CollectDataWorkV1, CompletedCollectDataV1, execute_selected_collect_data_v1,
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
    /// The caller runs genuine admission and Rule first, and re-reads Current
    /// admission after Rule/Modify contributions. Unauthenticated CollectData
    /// has no economy resolver, selective Accepted audit or factory followup.
    /// General Rule cost is global; business electricity is the collector's
    /// agent ledger and remains exclusively with the existing typed owner.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn execute_collect_data_after_rules_v1<P, ER, CR, PE, PC>(
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
        data_work: &mut CollectDataWorkV1,
        audit_work: &mut oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        tail_work: &mut crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<CompletedCollectDataV1, OrdinaryDriverErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        ER: FnMut() -> Option<ValueTreeError>,
        CR: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        let result: Result<_, OrdinaryDriverErrorV1<O::Error>> = (|| {
            let data_family =
                |action: &crate::Action| matches!(action, crate::Action::CollectData { .. });
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
                        "CollectData driver context pool/class",
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
            if !data_family(&selected.envelope().action)
                || !data_family(effective)
                || !outcome.uses_budget(self.ctx.values.memory)
            {
                return Err(OrdinaryDriverErrorV1::Module(
                    RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                        "CollectData driver action/rule/pool binding",
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
                    CompletedCollectDataV1::from_rejection(
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
                    execute_selected_collect_data_v1(
                        &mut step,
                        materials,
                        selected,
                        outcome,
                        self.families.agents,
                        self.families.roots,
                        read,
                        parent_read,
                        data_work,
                        audit_work,
                        self.events.max_event_bytes,
                        &guard,
                        &live,
                        &mut self.events.persist_original,
                    )
                    .map_err(RootModuleExecutionErrorV1::Body)?
                }
            };
            let event = PersistedWorldEventV1::from_collect_data(&completed);
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
