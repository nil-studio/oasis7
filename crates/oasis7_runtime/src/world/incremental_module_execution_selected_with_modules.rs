//! Complete selected FIFO action lane, including pre-Rule economy modules.
use super::nonmovement_driver::{
    CompletedNonmovementActionV1, NonmovementSelectedErrorV1, NonmovementSelectedWorkV1,
};
use super::ordinary_settlement::OrdinaryDriverErrorV1;
use super::*;
use crate::world::incremental_action_execution::SelectedPendingActionV1;
pub(in crate::world) enum CompletedSelectedNonmovementV2 {
    Rules {
        outcome: ReservedValue<RootRulePhaseOutcomeV1>,
        completed: CompletedNonmovementActionV1,
    },
    ModuleRejected(super::factory_module_rejection::CompletedFactoryModuleRejectionV1),
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
    pub(in crate::world) fn execute_selected_nonmovement_with_modules_v1<
        P,
        ER,
        CR,
        PE,
        PC,
        AP,
        AE,
        AC,
    >(
        &mut self,
        selected: &SelectedPendingActionV1,
        qualification: &ReservedValue<
            oasis7_world_state::incremental_runtime_revocation::RuntimeRevocationQualificationV1,
        >,
        current: &oasis7_world_state::incremental_runtime_revocation::RuntimeRevocationCurrentV1,
        admission_read: &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<
            '_,
            AP,
            R,
            AE,
            AC,
        >,
        policy_scan: &mut StateScanBudget,
        policy_work: &mut oasis7_world_state::incremental_runtime_revocation::RevocationReadWorkV1,
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
        mut work: NonmovementSelectedWorkV1<'_>,
        common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
        audit_work: &mut oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        rule_work: &mut crate::world::incremental_module_invocation::RootRulePhaseWorkV1,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        tail_work: &mut crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<CompletedSelectedNonmovementV2, NonmovementSelectedErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        ER: FnMut() -> Option<ValueTreeError>,
        CR: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
        AP: MerkleNodeReader,
        AE: FnMut() -> Option<ValueTreeError>,
        AC: FnMut() -> bool,
    {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if !matches!(
                selected.envelope().action,
                crate::Action::BuildFactoryWithModule { .. }
                    | crate::Action::ScheduleRecipeWithModule { .. }
                    | crate::Action::ValidateProductWithModule { .. }
            ) {
                return self
                    .execute_selected_nonmovement_v1(
                        selected,
                        qualification,
                        current,
                        admission_read,
                        policy_scan,
                        policy_work,
                        materials,
                        read,
                        parent_read,
                        work,
                        audit_work,
                        directory,
                        rule_work,
                        enumeration,
                        tail_work,
                        live_owner,
                    )
                    .map(
                        |(outcome, completed)| CompletedSelectedNonmovementV2::Rules {
                            outcome,
                            completed,
                        },
                    );
            }
            let prepared = {
                let (factory, sources, core) = work.factory_module_parts_v1().ok_or_else(|| {
                    NonmovementSelectedErrorV1::Business(OrdinaryDriverErrorV1::Module(
                        RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                            "selected module actual work slot",
                        )),
                    ))
                })?;
                if core.identity() != common.work().identity() {
                    return Err(NonmovementSelectedErrorV1::Business(
                        OrdinaryDriverErrorV1::Module(RootModuleExecutionErrorV1::Body(
                            BodyMutationError::Invalid("selected module Common identity"),
                        )),
                    ));
                }
                self.prepare_selected_factory_module_rule_lane_v1(
                    selected,
                    qualification,
                    current,
                    admission_read,
                    policy_scan,
                    policy_work,
                    sources,
                    parent_read,
                    factory,
                    common,
                    audit_work,
                    directory,
                    rule_work,
                    live_owner,
                )
                .map_err(NonmovementSelectedErrorV1::Rules)?
            };
            match prepared {
                super::selected_factory_rule::PreparedSelectedFactoryRuleLaneV1::Rules {
                    outcome,
                    admission,
                } => {
                    self.execute_nonmovement_after_rules_v1(
                        selected,
                        outcome,
                        &admission,
                        admission_read,
                        policy_scan,
                        policy_work,
                        materials,
                        read,
                        parent_read,
                        work,
                        audit_work,
                        directory,
                        enumeration,
                        tail_work,
                        live_owner,
                    )
                    .map(|(outcome, completed)| {
                        CompletedSelectedNonmovementV2::Rules { outcome, completed }
                    })
                }
                super::selected_factory_rule::PreparedSelectedFactoryRuleLaneV1::Rejected {
                    outcome,
                    initial,
                } => {
                    let (factory, sources, _) =
                        work.factory_module_parts_v1().ok_or_else(|| {
                            NonmovementSelectedErrorV1::Business(OrdinaryDriverErrorV1::Module(
                                RootModuleExecutionErrorV1::Body(BodyMutationError::Invalid(
                                    "selected rejection actual work slot",
                                )),
                            ))
                        })?;
                    self.execute_factory_module_rejection_v1(
                        outcome,
                        initial,
                        sources,
                        factory,
                        materials,
                        read,
                        parent_read,
                        common,
                        None,
                        audit_work,
                        directory,
                        enumeration,
                        tail_work,
                        live_owner,
                    )
                    .map(CompletedSelectedNonmovementV2::ModuleRejected)
                    .map_err(NonmovementSelectedErrorV1::Business)
                }
            }
        }));
        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                Err(error)
            }
            Err(panic) => {
                let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                    self.ctx.transaction.contribute(|_| {
                        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                    });
                std::panic::resume_unwind(panic)
            }
        }
    }
    pub(in crate::world) fn execute_next_nonmovement_with_modules_v1<
        P,
        ER,
        CR,
        PE,
        PC,
        AP,
        AE,
        AC,
    >(
        &mut self,
        prologue: &crate::world::incremental_factory_phase::FactoryPhaseOutcomeV1,
        pending: &mut ReservedValue<
            oasis7_world_state::incremental_pending_action_overlay::PendingActionCurrentV1,
        >,
        pending_work: &mut oasis7_world_state::incremental_pending_action_family::PendingActionFamilyWorkV1,
        qualification: &ReservedValue<
            oasis7_world_state::incremental_runtime_revocation::RuntimeRevocationQualificationV1,
        >,
        current: &oasis7_world_state::incremental_runtime_revocation::RuntimeRevocationCurrentV1,
        admission_read: &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<
            '_,
            AP,
            R,
            AE,
            AC,
        >,
        policy_scan: &mut StateScanBudget,
        policy_work: &mut oasis7_world_state::incremental_runtime_revocation::RevocationReadWorkV1,
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
        work: NonmovementSelectedWorkV1<'_>,
        common: &mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>,
        audit_work: &mut oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        rule_work: &mut crate::world::incremental_module_invocation::RootRulePhaseWorkV1,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        tail_work: &mut crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<Option<CompletedSelectedNonmovementV2>, NonmovementSelectedErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        ER: FnMut() -> Option<ValueTreeError>,
        CR: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
        AP: MerkleNodeReader,
        AE: FnMut() -> Option<ValueTreeError>,
        AC: FnMut() -> bool,
        O::Error: std::error::Error + Send + Sync + 'static,
    {
        let result = (|| {
            let live = live_owner();
            prologue
                .validate_for_queue(
                    self.ctx.transaction,
                    self.events.world_events,
                    self.now,
                    self.ctx.values.memory,
                    &live,
                )
                .map_err(|error| {
                    NonmovementSelectedErrorV1::Business(OrdinaryDriverErrorV1::Module(
                        RootModuleExecutionErrorV1::Body(error),
                    ))
                })?;
            let guard = self.ctx.transaction.guard();
            let step = crate::world::incremental_step::IwsStepContext::borrow_counted_at(
                self.ctx.transaction,
                self.now,
                &guard,
                &live,
                self.events.values,
                &mut self.events.mutation,
                &mut self.events.original_work,
                self.events.world_events,
            )
            .map_err(|error| {
                NonmovementSelectedErrorV1::Business(OrdinaryDriverErrorV1::Module(
                    RootModuleExecutionErrorV1::Body(error),
                ))
            })?;
            drop(step);
            let Some(selected) =
                self.take_nonmovement_input_v1(pending, pending_work, parent_read, live_owner)?
            else {
                return Ok(None);
            };
            self.execute_selected_nonmovement_with_modules_v1(
                &selected,
                qualification,
                current,
                admission_read,
                policy_scan,
                policy_work,
                materials,
                read,
                parent_read,
                work,
                common,
                audit_work,
                directory,
                rule_work,
                enumeration,
                tail_work,
                live_owner,
            )
            .map(Some)
        })();
        if result.is_err() {
            let _ = self
                .ctx
                .transaction
                .contribute::<(), oasis7_formal_protocol::incremental_execution::ExecutionError>(
                |_| Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted),
            );
        }
        result
    }
}
