//! Fixed pre-Rule WithModule action producer. No accepted disposition or Rule
//! is manufactured here: the FIFO consumes the concrete resolution exactly once.
use super::*;
use crate::world::{
    incremental_action_execution::SelectedPendingActionV1,
    incremental_normal_action_factory_schedule::{
        FactoryScheduleSourcesV1, FactoryScheduleWorkV1, factory_module::*,
    },
};
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::{
    FormalBoundedContextV1, WorkIdentityV1,
};
use oasis7_world_state::incremental_field_read::WorldFieldReadContextV1;
pub(in crate::world) struct FailedFactoryModuleActionV1<'s> {
    call: PreparedFactoryModuleCallV1<'s>,
    execution: PaidFactoryModuleExecutionV1,
    reason: ReservedValue<crate::RejectReason>,
    guard: oasis7_formal_protocol::incremental_execution::PreparedGuard,
    core: WorkIdentityV1,
    now: u64,
}
impl FailedFactoryModuleActionV1<'_> {
    pub(in crate::world) fn selected(&self) -> &SelectedPendingActionV1 {
        self.call.selected()
    }
    pub(in crate::world) fn reason(&self) -> &crate::RejectReason {
        &self.reason
    }
    pub(in crate::world) fn execution(&self) -> &PaidFactoryModuleExecutionV1 {
        &self.execution
    }
    pub(in crate::world) fn call(&self) -> &PreparedFactoryModuleCallV1<'_> {
        &self.call
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.call
            .validate_evidence_identity(tx, common, now, live)?;
        self.execution
            .validate_evidence_identity(&self.call, tx, common, now, live)?;
        tx.validate_guard(&self.guard, live)?;
        if self.core != common.work().identity()
            || self.now != now
            || !self.reason.uses_budget(common.memory())
            || !matches!(
                self.execution.vm_failure(),
                Some(crate::WorldError::ModuleCallFailed { .. })
            )
        {
            return Err(BodyMutationError::Invalid(
                "factory actual VM failure source",
            ));
        }
        Ok(())
    }
}
pub(in crate::world) enum FactoryModuleBeforeRulesOutcomeV1<'s> {
    PreRejected(RejectedFactoryModuleActionV1<'s>),
    Resolved(ResolvedFactoryModuleActionV1<'s>),
    Denied(DeniedFactoryModuleOutputV1<'s>),
    Invalid(InvalidFactoryModuleOutputV1<'s>),
    Failed(FailedFactoryModuleActionV1<'s>),
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
    pub(in crate::world) fn resolve_selected_factory_module_before_rules_v1<'selected, P, PE, PC>(
        &mut self,
        selected: &'selected SelectedPendingActionV1,
        sources: &FactoryScheduleSourcesV1<'_>,
        parent: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        work: &mut FactoryScheduleWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<FactoryModuleBeforeRulesOutcomeV1<'selected>, RootModuleExecutionErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let prepared = prepare_factory_module_action_v1(
                selected,
                sources,
                self.ctx,
                parent,
                work,
                common,
                self.now,
                &live_owner(),
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            let call = match prepared {
                PreparedFactoryModuleActionV1::Rejected(value) => {
                    return Ok(FactoryModuleBeforeRulesOutcomeV1::PreRejected(value));
                }
                PreparedFactoryModuleActionV1::Call(value) => value,
            };
            if call.kind() == FactoryModuleCallKindV1::Build {
                let (mut outcome, evidence) = self.with_staged_module_branch_v1(
                    common,
                    live_owner,
                    move |root, common, live_owner| {
                        let execution =
                            root.execute_factory_module_request_v1(&call, common, live_owner)?;
                        if let Some(error) = execution.vm_failure() {
                            let reason = format_failure_rejection_v1(error, common)
                                .map_err(RootModuleExecutionErrorV1::Body)?;
                            let failed = FailedFactoryModuleActionV1 {
                                call,
                                execution,
                                reason,
                                guard: root.ctx.transaction.guard(),
                                core: common.work().identity(),
                                now: root.now,
                            };
                            failed
                                .validate(root.ctx.transaction, common, root.now, &live_owner())
                                .map_err(RootModuleExecutionErrorV1::Body)?;
                            return Ok(super::StagedRootModuleDispositionV1::Discard(
                                FactoryModuleBeforeRulesOutcomeV1::Failed(failed),
                            ));
                        }
                        let resolved = resolve_actual_factory_module_output_v1(
                            call,
                            execution,
                            root.ctx.transaction,
                            common,
                            root.now,
                            &live_owner(),
                        )
                        .map_err(RootModuleExecutionErrorV1::Body)?;
                        let resolved = match resolved {
                            ResolvedFactoryModuleOutcomeV1::Resolved(value) => value
                                .preflight_build_v1(
                                    sources,
                                    root.ctx,
                                    parent,
                                    work,
                                    common,
                                    root.now,
                                    &live_owner(),
                                )
                                .map_err(RootModuleExecutionErrorV1::Body)?,
                            other => other,
                        };
                        Ok(match resolved {
                            ResolvedFactoryModuleOutcomeV1::Resolved(value) => {
                                super::StagedRootModuleDispositionV1::Commit(
                                    FactoryModuleBeforeRulesOutcomeV1::Resolved(value),
                                )
                            }
                            ResolvedFactoryModuleOutcomeV1::Denied(value) => {
                                super::StagedRootModuleDispositionV1::Discard(
                                    FactoryModuleBeforeRulesOutcomeV1::Denied(value),
                                )
                            }
                            ResolvedFactoryModuleOutcomeV1::Invalid(value) => {
                                super::StagedRootModuleDispositionV1::Discard(
                                    FactoryModuleBeforeRulesOutcomeV1::Invalid(value),
                                )
                            }
                        })
                    },
                )?;
                outcome
                    .rejoin_build_v1(
                        &evidence,
                        self.ctx.transaction,
                        common,
                        self.now,
                        &live_owner(),
                    )
                    .map_err(RootModuleExecutionErrorV1::Body)?;
                return Ok(outcome);
            }
            let execution = self.execute_factory_module_request_v1(&call, common, live_owner)?;
            if let Some(error) = execution.vm_failure() {
                let detail=crate::world::incremental_normal_action_factory_schedule::factory_module::format_failure_rejection_v1(error,common).map_err(RootModuleExecutionErrorV1::Body)?;
                return Ok(FactoryModuleBeforeRulesOutcomeV1::Failed(
                    FailedFactoryModuleActionV1 {
                        call,
                        execution,
                        reason: detail,
                        guard: self.ctx.transaction.guard(),
                        core: common.work().identity(),
                        now: self.now,
                    },
                ));
            }
            let resolved = resolve_actual_factory_module_output_v1(
                call,
                execution,
                self.ctx.transaction,
                common,
                self.now,
                &live_owner(),
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            Ok(match resolved {
                ResolvedFactoryModuleOutcomeV1::Resolved(v) => {
                    FactoryModuleBeforeRulesOutcomeV1::Resolved(v)
                }
                ResolvedFactoryModuleOutcomeV1::Denied(v) => {
                    FactoryModuleBeforeRulesOutcomeV1::Denied(v)
                }
                ResolvedFactoryModuleOutcomeV1::Invalid(v) => {
                    FactoryModuleBeforeRulesOutcomeV1::Invalid(v)
                }
            })
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
}

impl FactoryModuleBeforeRulesOutcomeV1<'_> {
    fn rejoin_build_v1<M: MerkleNodeReader>(
        &mut self,
        evidence: &oasis7_formal_protocol::incremental_execution::StagedBranchRejoinEvidenceV1,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        match self {
            Self::Resolved(value) => value.rejoin_build_v1(evidence, tx, common, now, live),
            Self::Denied(value) => value.rejoin_build_v1(evidence, tx, common, now, live),
            Self::Invalid(value) => value.rejoin_build_v1(evidence, tx, common, now, live),
            Self::Failed(value) => {
                if evidence.disposition()!=oasis7_formal_protocol::incremental_execution::StagedBranchDispositionV1::Discarded||value.core!=common.work().identity()||value.now!=now||!value.reason.uses_budget(common.memory())||!matches!(value.execution.vm_failure(),Some(crate::WorldError::ModuleCallFailed{..})){return Err(BodyMutationError::Invalid("factory failed rejoin binding"))}
                let guard = evidence
                    .rebind_source_guard(&value.guard, tx, common, live)
                    .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
                value.execution.rejoin_build_execution_v1(
                    &value.call,
                    evidence,
                    tx,
                    common,
                    now,
                    live,
                )?;
                value.guard = guard;
                value.validate(tx, common, now, live)
            }
            Self::PreRejected(_) => Err(BodyMutationError::Invalid(
                "factory prerejection never enters staged VM",
            )),
        }
    }
}
