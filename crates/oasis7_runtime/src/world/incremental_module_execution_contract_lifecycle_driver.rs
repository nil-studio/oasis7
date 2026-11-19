//! Original Rule/fees/selective Accepted/Post around Open/SettleContract Current CAS.
use super::ordinary_settlement::{OrdinaryDriverErrorV1, settle_ordinary_rules_v1};
use super::*;
use crate::world::{
    incremental_action_admission::UnboundActionAdmissionV1,
    incremental_action_execution::SelectedPendingActionV1,
    incremental_module_execution_event::PersistedWorldEventV1,
    incremental_module_invocation::RootRulePhaseOutcomeV1,
    incremental_normal_action_diplomacy::{
        CompletedAllianceActionV1,
        contracts::{
            ContractSourcesV1, ContractWorkV1, commit_contract_action_v1,
            prepare_contract_action_v1,
        },
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
    /// Genuine pre/post-Rule admission is supplied by the selected lane. The
    /// private owner preflights business before the genuine selective Accepted.
    /// The actual result follows that audit, including rules/fee rejections.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn execute_contract_after_rules_v1<P, ER, CR, PE, PC>(
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
        work: &mut ContractWorkV1,
        sources: &ContractSourcesV1<'_, '_>,
        common_work: &FormalBoundedWorkV1,
        accepted_entries: Option<&mut usize>,
        audit_work: &mut oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        tail_work: &mut crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<CompletedAllianceActionV1, OrdinaryDriverErrorV1<O::Error>>
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
                crate::Action::OpenEconomicContract { .. }
                    | crate::Action::SettleEconomicContract { .. }
            ) || !outcome.uses_budget(self.ctx.values.memory)
                || !self
                    .events
                    .mutation
                    .memory
                    .same_pool(self.ctx.values.memory)
                || !self.members.memory.same_pool(self.ctx.values.memory)
                || self.ctx.values.class != MemoryClass::Ordinary
            {
                return Err(RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                    "Alliance driver source",
                ))
                .into());
            }
            let rejected_layout = self
                .ctx
                .values
                .memory
                .reserve(
                    std::mem::size_of::<CompletedAllianceActionV1>(),
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
            let prepared = if rejection.is_some() {
                None
            } else {
                let mut common = FormalBoundedContextV1::new(
                    self.ctx.values.memory,
                    common_work,
                    &mut self.ctx.cancelled,
                );
                let mut overlay =
                    oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1 {
                        transaction: &mut *self.ctx.transaction,
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
                Some(
                    prepare_contract_action_v1(
                        selected,
                        outcome,
                        sources,
                        &mut overlay,
                        parent_read,
                        work,
                        &mut common,
                        self.now,
                        &live,
                    )
                    .map_err(RootModuleExecutionErrorV1::Body)?,
                )
            };
            let accepted = self.append_selected_accepted_disposition_v1(
                selected,
                materials,
                self.families.agents,
                read,
                parent_read,
                accepted_entries.unwrap_or(&mut work.shared.pending_entries_remaining),
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
                    self.events.world_events,
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
                    CompletedAllianceActionV1::Rejected {
                        accepted: accepted.into_audit(),
                        event,
                        _fees: std::array::from_fn(|_| None),
                        _layout: rejected_layout,
                    }
                } else {
                    commit_contract_action_v1(
                        prepared.ok_or_else(|| {
                            RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                                "Contract private prepare",
                            ))
                        })?,
                        accepted,
                        &mut step,
                        read,
                        parent_read,
                        self.members,
                        work,
                        &mut common,
                        materials,
                        self.events.max_event_bytes,
                        audit_work,
                        &live,
                        &mut self.events.persist_original,
                    )
                    .map_err(RootModuleExecutionErrorV1::Body)?
                }
            };
            let event = PersistedWorldEventV1::from_alliance(&completed);
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
