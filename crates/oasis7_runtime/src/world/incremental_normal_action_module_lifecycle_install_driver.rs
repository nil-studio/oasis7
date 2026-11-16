//! Complete fixed install orchestration. Four original install Action variants
//! share this consumer; neither a DTO nor a caller verdict bypasses governance.
use super::*;
use super::{
    governance_execute::*, governance_finality::*, instance_execute::*, instance_plan::*,
    instance_tail_rows::*,
};
use crate::world::{
    incremental_action_execution::SelectedActionAuditV1,
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_module_invocation::{AcceptedInvocationDirectoryV1, SelectedAcceptedDispositionV1},
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::{
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext},
};
use oasis7_world_state::{
    incremental_field_member::MemberRecordWriteContext,
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
};
macro_rules! overlay {
    ($step:expr,$read:expr) => {
        WorldFieldOverlayContextV1 {
            transaction: &mut *$step.transaction,
            values: ValueReadContext {
                source: $read.values.source,
                memory: $read.values.memory,
                class: $read.values.class,
                io: &mut *$read.values.io,
            },
            key_work: &mut *$read.key_work,
            semantic_bytes: $read.semantic_bytes,
            record_bytes: $read.record_bytes,
            take_state_error: &mut $read.take_state_error,
            cancelled: &mut $read.cancelled,
        }
    };
}

#[derive(Clone, Copy)]
enum CurrentInstanceActionKindV1 {
    Install,
    Upgrade,
    Rollback,
    Release,
}
enum CurrentSelectedInstanceDecisionV1 {
    Release {
        source: super::release_facts::ValidatedCurrentReleaseApplyV1,
        decision: CurrentSelectedInstallDecisionV1,
    },
    Install(CurrentSelectedInstallDecisionV1),
    Upgrade(super::upgrade_execute::CurrentSelectedUpgradeDecisionV1),
    Rollback(super::rollback_execute::CurrentSelectedRollbackDecisionV1),
}
pub(super) enum CurrentInstallFailureCauseV1 {
    ReleasePrelude {
        decision: CurrentSelectedInstallDecisionV1,
        _source: super::release_facts::ValidatedCurrentReleaseApplyV1,
    },
    Prelude(CurrentSelectedInstallDecisionV1),
    UpgradePrelude(super::upgrade_execute::CurrentSelectedUpgradeDecisionV1),
    RollbackPrelude(super::rollback_execute::CurrentSelectedRollbackDecisionV1),
    Rows(CurrentModuleInstallRowsDecisionV1),
    Sequence {
        error: ReservedValue<crate::WorldError>,
        _facts: Option<super::instance_current::ModuleInstanceFactsFeesV1>,
        stage: CurrentModuleGovernanceFailureStageV1,
    },
    Local {
        error: crate::WorldError,
        _fees: crate::world::ModuleCapabilityAdmissionFeesV1,
    },
    Prefix(ModuleApplyPrefixDecisionV1),
    Certificate(CurrentModuleApplyValidationV1),
}
/// Only this fixed driver creates the failure source, retaining the genuine
/// phase owner and early publications. The event layer never accepts a reason
/// string or an untyped caller decision as publication authority.
pub(super) struct CurrentSelectedInstallFailureV1 {
    pub(super) cause: CurrentInstallFailureCauseV1,
    operation: CurrentInstanceActionKindV1,
    pub(super) accepted: Option<SelectedActionAuditV1>,
    pub(super) first: Option<super::event::PreparedNormalModuleLifecycleEventV1>,
    pub(super) last: Option<super::event::PreparedNormalModuleLifecycleEventV1>,
    _seed: Option<CurrentInstallTailSeedV1>,
    _rows: Option<PreparedCurrentModuleInstallRowsV1>,
    _local: Option<crate::world::PaidCurrentLocalModuleGovernanceFinalityV1>,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    now: u64,
    action_id: u64,
    _fee: MemoryReservation,
}
impl CurrentSelectedInstallFailureV1 {
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        memory: &oasis7_proto::memory_budget::MemoryBudget,
        core: WorkIdentityV1,
        now: u64,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_transaction_identity(&self.guard)?;
        tx.validate_guard(&tx.guard(), live)?;
        if self.core != core || self.now != now || !self._fee.uses_budget(memory) {
            return Err(invalid());
        }
        if self.prelude_reason_v1().is_none()
            && (self.world_error_v1().is_none() || self.reason_prefix_v1().is_none())
        {
            return Err(invalid());
        }
        if let Some(a) = &self.accepted {
            a.validate(tx)?;
            if !a.uses_budget(memory) {
                return Err(invalid());
            }
        }
        for event in [&self.first, &self.last].into_iter().flatten() {
            event.validate(tx, memory, now, core, live)?;
        }
        Ok(())
    }
    pub(super) fn action_id(&self) -> u64 {
        self.action_id
    }
    pub(super) fn into_business_v1(self) -> Result<Self, BodyMutationError> {
        if matches!(
            &self.cause,
            CurrentInstallFailureCauseV1::Rows(CurrentModuleInstallRowsDecisionV1::Rejected { .. })
        ) {
            return Err(BodyMutationError::Original(Box::new(self)));
        }
        Ok(self)
    }
    pub(super) fn prelude_reason_v1(&self) -> Option<&crate::RejectReason> {
        match &self.cause {
            CurrentInstallFailureCauseV1::ReleasePrelude {
                decision: CurrentSelectedInstallDecisionV1::Rejected { reason, .. },
                ..
            }
            | CurrentInstallFailureCauseV1::Prelude(CurrentSelectedInstallDecisionV1::Rejected {
                reason,
                ..
            })
            | CurrentInstallFailureCauseV1::UpgradePrelude(
                super::upgrade_execute::CurrentSelectedUpgradeDecisionV1::Rejected {
                    reason, ..
                },
            )
            | CurrentInstallFailureCauseV1::RollbackPrelude(
                super::rollback_execute::CurrentSelectedRollbackDecisionV1::Rejected {
                    reason, ..
                },
            ) => Some(reason),
            _ => None,
        }
    }
    pub(super) fn world_error_v1(&self) -> Option<&crate::WorldError> {
        match &self.cause {
            CurrentInstallFailureCauseV1::Rows(CurrentModuleInstallRowsDecisionV1::Rejected {
                error,
                ..
            }) => Some(error),
            CurrentInstallFailureCauseV1::Sequence { error, .. } => Some(error),
            CurrentInstallFailureCauseV1::Local { error, .. } => Some(error),
            CurrentInstallFailureCauseV1::Prefix(ModuleApplyPrefixDecisionV1::Rejected {
                error,
                ..
            }) => Some(error),
            CurrentInstallFailureCauseV1::Certificate(
                CurrentModuleApplyValidationV1::Rejected { error, .. },
            ) => Some(error),
            _ => None,
        }
    }
    pub(super) fn reason_prefix_v1(&self) -> Option<&'static str> {
        match (&self.operation, &self.cause) {
            (
                CurrentInstanceActionKindV1::Rollback,
                CurrentInstallFailureCauseV1::Sequence {
                    stage: CurrentModuleGovernanceFailureStageV1::Shadow,
                    ..
                },
            ) => Some("shadow module rollback rejected: "),
            (
                CurrentInstanceActionKindV1::Rollback,
                CurrentInstallFailureCauseV1::Sequence {
                    stage: CurrentModuleGovernanceFailureStageV1::Approve,
                    ..
                },
            ) => Some("approve module rollback rejected: "),
            (
                CurrentInstanceActionKindV1::Rollback,
                CurrentInstallFailureCauseV1::Local { .. }
                | CurrentInstallFailureCauseV1::Prefix(_)
                | CurrentInstallFailureCauseV1::Certificate(_),
            ) => Some("apply module rollback rejected: "),
            (
                CurrentInstanceActionKindV1::Install | CurrentInstanceActionKindV1::Release,
                CurrentInstallFailureCauseV1::Sequence {
                    stage: CurrentModuleGovernanceFailureStageV1::Shadow,
                    ..
                },
            ) => Some("shadow module install rejected: "),
            (
                CurrentInstanceActionKindV1::Upgrade,
                CurrentInstallFailureCauseV1::Sequence {
                    stage: CurrentModuleGovernanceFailureStageV1::Shadow,
                    ..
                },
            ) => Some("shadow module upgrade rejected: "),
            (
                CurrentInstanceActionKindV1::Install | CurrentInstanceActionKindV1::Release,
                CurrentInstallFailureCauseV1::Sequence {
                    stage: CurrentModuleGovernanceFailureStageV1::Approve,
                    ..
                },
            ) => Some("approve module install rejected: "),
            (
                CurrentInstanceActionKindV1::Upgrade,
                CurrentInstallFailureCauseV1::Sequence {
                    stage: CurrentModuleGovernanceFailureStageV1::Approve,
                    ..
                },
            ) => Some("approve module upgrade rejected: "),
            (
                CurrentInstanceActionKindV1::Install | CurrentInstanceActionKindV1::Release,
                CurrentInstallFailureCauseV1::Local { .. }
                | CurrentInstallFailureCauseV1::Prefix(_)
                | CurrentInstallFailureCauseV1::Certificate(_),
            ) => Some("apply module install rejected: "),
            (
                CurrentInstanceActionKindV1::Upgrade,
                CurrentInstallFailureCauseV1::Local { .. }
                | CurrentInstallFailureCauseV1::Prefix(_)
                | CurrentInstallFailureCauseV1::Certificate(_),
            ) => Some("apply module upgrade rejected: "),
            _ => None,
        }
    }
}
impl std::fmt::Debug for CurrentSelectedInstallFailureV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.world_error_v1() {
            Some(error) => std::fmt::Debug::fmt(error, f),
            None => f.write_str("selected install business rejection"),
        }
    }
}
impl std::fmt::Display for CurrentSelectedInstallFailureV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}
impl std::error::Error for CurrentSelectedInstallFailureV1 {}
pub(super) enum ExecutedCurrentSelectedInstallV1 {
    Ready {
        plan: SelectedCurrentModuleInstallPlanV1,
        accepted: Option<SelectedActionAuditV1>,
        first: Option<super::event::PreparedNormalModuleLifecycleEventV1>,
        last: Option<super::event::PreparedNormalModuleLifecycleEventV1>,
    },
    Rejected(CurrentSelectedInstallFailureV1),
}
enum InstallCertificateV1<'a> {
    Explicit(&'a crate::GovernanceFinalityCertificate),
    Local(crate::world::PaidCurrentLocalModuleGovernanceFinalityV1),
}
#[allow(clippy::too_many_arguments)]
pub(super) fn execute_current_selected_install_v1<M, P, V, W, D, E, C, PE, PC, CW, ME, F>(
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    accepted: SelectedAcceptedDispositionV1,
    sources: &ModuleLifecycleSourcesV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    module: &mut ModuleLifecycleWorkV1<'_>,
    factory: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
    max_event_bytes: usize,
    persist: &mut F,
    live: &ExecutionFence,
) -> Result<ExecutedCurrentSelectedInstallV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    execute_current_instance_origin_v1(
        None,
        selected,
        outcome,
        accepted,
        sources,
        step,
        read,
        parent,
        write,
        materials,
        module,
        factory,
        common,
        directory,
        max_event_bytes,
        persist,
        live,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_current_release_install_v1<M, P, V, W, D, E, C, PE, PC, CW, ME, F>(
    release: super::release_facts::ValidatedCurrentReleaseApplyV1,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    accepted: SelectedAcceptedDispositionV1,
    sources: &ModuleLifecycleSourcesV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    module: &mut ModuleLifecycleWorkV1<'_>,
    factory: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
    max_event_bytes: usize,
    persist: &mut F,
    live: &ExecutionFence,
) -> Result<ExecutedCurrentSelectedInstallV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    execute_current_instance_origin_v1(
        Some(release),
        selected,
        outcome,
        accepted,
        sources,
        step,
        read,
        parent,
        write,
        materials,
        module,
        factory,
        common,
        directory,
        max_event_bytes,
        persist,
        live,
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_current_instance_origin_v1<M, P, V, W, D, E, C, PE, PC, CW, ME, F>(
    release: Option<super::release_facts::ValidatedCurrentReleaseApplyV1>,
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    accepted: SelectedAcceptedDispositionV1,
    sources: &ModuleLifecycleSourcesV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    module: &mut ModuleLifecycleWorkV1<'_>,
    factory: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
    max_event_bytes: usize,
    persist: &mut F,
    live: &ExecutionFence,
) -> Result<ExecutedCurrentSelectedInstallV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        accepted.validate(selected, step.transaction, common.memory())?;
        let now = step.time();
        pay(
            common,
            std::mem::size_of::<CurrentSelectedInstallFailureV1>(),
        )?;
        let failure_fee = common
            .memory()
            .reserve(
                std::mem::size_of::<CurrentSelectedInstallFailureV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let effective = outcome
            .override_action_for_selected_v1(selected, step.transaction)?
            .unwrap_or(&selected.envelope().action);
        if release.is_some()
            && !matches!(
                effective,
                crate::Action::ModuleReleaseApply { .. }
                    | crate::Action::ModuleReleaseApplyWithFinality { .. }
            )
        {
            return Err(invalid());
        }
        let operation = match effective {
            crate::Action::InstallModuleFromArtifact { .. }
            | crate::Action::InstallModuleFromArtifactWithFinality { .. }
            | crate::Action::InstallModuleToTargetFromArtifact { .. }
            | crate::Action::InstallModuleToTargetFromArtifactWithFinality { .. } => {
                CurrentInstanceActionKindV1::Install
            }
            crate::Action::UpgradeModuleFromArtifact { .. }
            | crate::Action::UpgradeModuleFromArtifactWithFinality { .. } => {
                CurrentInstanceActionKindV1::Upgrade
            }
            crate::Action::RollbackModuleInstance { .. }
            | crate::Action::RollbackModuleInstanceWithFinality { .. } => {
                CurrentInstanceActionKindV1::Rollback
            }
            crate::Action::ModuleReleaseApply { .. }
            | crate::Action::ModuleReleaseApplyWithFinality { .. }
                if release.is_some() =>
            {
                CurrentInstanceActionKindV1::Release
            }
            _ => return Err(invalid()),
        };
        let decision = {
            let mut ctx = overlay!(step, read);
            match operation {
                CurrentInstanceActionKindV1::Release => {
                    let source = release.ok_or_else(invalid)?;
                    prepare_current_release_install_v1(
                        &source, selected, outcome, &accepted, sources, &mut ctx, parent, factory,
                        module, common, directory, live,
                    )
                    .map(|decision| CurrentSelectedInstanceDecisionV1::Release { source, decision })
                }
                CurrentInstanceActionKindV1::Install => prepare_current_selected_install_v1(
                    selected, outcome, &accepted, sources, &mut ctx, parent, factory, module,
                    common, directory, live,
                )
                .map(CurrentSelectedInstanceDecisionV1::Install),
                CurrentInstanceActionKindV1::Upgrade => {
                    super::upgrade_execute::prepare_current_selected_upgrade_v1(
                        selected, outcome, &accepted, sources, &mut ctx, parent, factory, module,
                        common, directory, live,
                    )
                    .map(CurrentSelectedInstanceDecisionV1::Upgrade)
                }
                CurrentInstanceActionKindV1::Rollback => {
                    super::rollback_execute::prepare_current_selected_rollback_v1(
                        selected, outcome, &accepted, sources, &mut ctx, parent, factory, module,
                        common, directory, live,
                    )
                    .map(CurrentSelectedInstanceDecisionV1::Rollback)
                }
            }
        }
        .map_err(source_error)?;
        macro_rules! rejected {
            ($cause:expr,$accepted:expr,$first:expr,$last:expr,$seed:expr,$rows:expr,$local:expr) => {
                return Ok(ExecutedCurrentSelectedInstallV1::Rejected(
                    CurrentSelectedInstallFailureV1 {
                        cause: $cause,
                        operation,
                        accepted: $accepted,
                        first: $first,
                        last: $last,
                        _seed: $seed,
                        _rows: $rows,
                        _local: $local,
                        guard: step.transaction.guard(),
                        core: common.work().identity(),
                        now,
                        action_id: selected.envelope().id,
                        _fee: failure_fee,
                    },
                ))
            };
        }
        let decision = match decision {
            CurrentSelectedInstanceDecisionV1::Release { source, decision }
                if matches!(&decision, CurrentSelectedInstallDecisionV1::Rejected { .. }) =>
            {
                rejected!(
                    CurrentInstallFailureCauseV1::ReleasePrelude {
                        decision,
                        _source: source
                    },
                    accepted.into_audit(),
                    None,
                    None,
                    None,
                    None,
                    None
                );
            }
            CurrentSelectedInstanceDecisionV1::Install(decision)
                if matches!(&decision, CurrentSelectedInstallDecisionV1::Rejected { .. }) =>
            {
                rejected!(
                    CurrentInstallFailureCauseV1::Prelude(decision),
                    accepted.into_audit(),
                    None,
                    None,
                    None,
                    None,
                    None
                );
            }
            CurrentSelectedInstanceDecisionV1::Upgrade(decision)
                if matches!(
                    &decision,
                    super::upgrade_execute::CurrentSelectedUpgradeDecisionV1::Rejected { .. }
                ) =>
            {
                rejected!(
                    CurrentInstallFailureCauseV1::UpgradePrelude(decision),
                    accepted.into_audit(),
                    None,
                    None,
                    None,
                    None,
                    None
                );
            }
            CurrentSelectedInstanceDecisionV1::Rollback(decision)
                if matches!(
                    &decision,
                    super::rollback_execute::CurrentSelectedRollbackDecisionV1::Rejected { .. }
                ) =>
            {
                rejected!(
                    CurrentInstallFailureCauseV1::RollbackPrelude(decision),
                    accepted.into_audit(),
                    None,
                    None,
                    None,
                    None,
                    None
                );
            }
            decision => decision,
        };
        let decision = match decision {
            CurrentSelectedInstanceDecisionV1::Release {
                source,
                decision:
                    CurrentSelectedInstallDecisionV1::NoGovernance {
                        manifest,
                        hash,
                        facts,
                        ..
                    },
            } => {
                let seed = take_release_install_seed_v1(
                    source,
                    facts,
                    selected,
                    outcome,
                    step.transaction,
                    common,
                    live,
                )?;
                let governance = CurrentInstallGovernanceSourceV1::Unchanged { manifest, hash };
                let rows = prepare_current_module_install_rows_v1(
                    &seed, selected, outcome, sources, step, read, parent, module, factory, common,
                    live,
                )?;
                let rows = match rows {
                    CurrentModuleInstallRowsDecisionV1::Ready(rows) => rows,
                    rejected => {
                        rejected!(
                            CurrentInstallFailureCauseV1::Rows(rejected),
                            accepted.into_audit(),
                            None,
                            None,
                            Some(seed),
                            None,
                            None
                        );
                    }
                };
                let plan = bind_current_install_tail_v1(
                    seed,
                    rows,
                    governance,
                    selected,
                    outcome,
                    step.transaction,
                    common,
                    now,
                    live,
                )?;
                return Ok(ExecutedCurrentSelectedInstallV1::Ready {
                    plan,
                    accepted: accepted.into_audit(),
                    first: None,
                    last: None,
                });
            }
            CurrentSelectedInstanceDecisionV1::Install(decision)
                if matches!(
                    &decision,
                    CurrentSelectedInstallDecisionV1::NoGovernance { .. }
                ) =>
            {
                let (seed, governance) = take_unchanged_install_seed_v1(
                    decision,
                    selected,
                    outcome,
                    step.transaction,
                    common,
                    live,
                )?;
                let rows = prepare_current_module_install_rows_v1(
                    &seed, selected, outcome, sources, step, read, parent, module, factory, common,
                    live,
                )?;
                let rows = match rows {
                    CurrentModuleInstallRowsDecisionV1::Ready(rows) => rows,
                    rejected => {
                        rejected!(
                            CurrentInstallFailureCauseV1::Rows(rejected),
                            accepted.into_audit(),
                            None,
                            None,
                            Some(seed),
                            None,
                            None
                        );
                    }
                };
                let plan = bind_current_install_tail_v1(
                    seed,
                    rows,
                    governance,
                    selected,
                    outcome,
                    step.transaction,
                    common,
                    now,
                    live,
                )?;
                return Ok(ExecutedCurrentSelectedInstallV1::Ready {
                    plan,
                    accepted: accepted.into_audit(),
                    first: None,
                    last: None,
                });
            }
            decision => decision,
        };
        let (update, seed) = match decision {
            CurrentSelectedInstanceDecisionV1::Release {
                source,
                decision: CurrentSelectedInstallDecisionV1::Governed { update, facts, .. },
            } => {
                let seed = take_release_install_seed_v1(
                    source,
                    facts,
                    selected,
                    outcome,
                    step.transaction,
                    common,
                    live,
                )?;
                (update, seed)
            }
            CurrentSelectedInstanceDecisionV1::Release { .. } => return Err(invalid()),
            CurrentSelectedInstanceDecisionV1::Install(decision) => take_governed_install_seed_v1(
                decision,
                selected,
                outcome,
                step.transaction,
                common,
                live,
            )?,
            CurrentSelectedInstanceDecisionV1::Upgrade(decision) => take_governed_upgrade_seed_v1(
                decision,
                selected,
                outcome,
                step.transaction,
                common,
                live,
            )?,
            CurrentSelectedInstanceDecisionV1::Rollback(decision) => {
                take_governed_rollback_seed_v1(
                    decision,
                    selected,
                    outcome,
                    step.transaction,
                    common,
                    live,
                )?
            }
        };
        let sequence = execute_current_module_governance_sequence_v1(
            update,
            selected,
            outcome,
            accepted,
            sources,
            step,
            read,
            parent,
            write,
            materials,
            module,
            factory,
            common,
            directory,
            max_event_bytes,
            persist,
            live,
        )?;
        let sequence = match sequence {
            CurrentModuleGovernanceSequenceV1::Queued(s) => s,
            CurrentModuleGovernanceSequenceV1::Rejected {
                first,
                last,
                error,
                _facts,
                stage,
            } => {
                rejected!(
                    CurrentInstallFailureCauseV1::Sequence {
                        error,
                        _facts,
                        stage
                    },
                    None,
                    Some(first),
                    last,
                    Some(seed),
                    None,
                    None
                );
            }
        };
        let action = outcome
            .override_action_for_selected_v1(selected, step.transaction)?
            .unwrap_or(&selected.envelope().action);
        let explicit = match action {
            crate::Action::InstallModuleFromArtifactWithFinality {
                finality_certificate,
                ..
            }
            | crate::Action::InstallModuleToTargetFromArtifactWithFinality {
                finality_certificate,
                ..
            }
            | crate::Action::UpgradeModuleFromArtifactWithFinality {
                finality_certificate,
                ..
            }
            | crate::Action::RollbackModuleInstanceWithFinality {
                finality_certificate,
                ..
            }
            | crate::Action::ModuleReleaseApplyWithFinality {
                finality_certificate,
                ..
            } => Some(finality_certificate),
            crate::Action::InstallModuleFromArtifact { .. }
            | crate::Action::InstallModuleToTargetFromArtifact { .. }
            | crate::Action::UpgradeModuleFromArtifact { .. }
            | crate::Action::RollbackModuleInstance { .. }
            | crate::Action::ModuleReleaseApply { .. } => None,
            _ => return Err(invalid()),
        };
        // The old local path builds before apply preconditions; the explicit path
        // starts with those preconditions. This ordering preserves the first refusal.
        let certificate = if let Some(certificate) = explicit {
            InstallCertificateV1::Explicit(certificate)
        } else {
            let counter = step.world_event_counter_v1().cloned().ok_or_else(invalid)?;
            let result = {
                let mut ctx = overlay!(step, read);
                crate::world::build_current_local_module_governance_finality_v1(
                    sequence.proposal_id,
                    &counter,
                    sources,
                    &mut ctx,
                    parent,
                    factory,
                    module,
                    common,
                    live,
                )
            };
            match result {
                Ok(local) => InstallCertificateV1::Local(local),
                Err(crate::world::CurrentModuleGovernanceFinalityErrorV1::Body(e)) => {
                    return Err(e);
                }
                Err(crate::world::CurrentModuleGovernanceFinalityErrorV1::World {
                    error,
                    fees,
                }) => {
                    rejected!(
                        CurrentInstallFailureCauseV1::Local { error, _fees: fees },
                        None,
                        Some(sequence.first),
                        Some(sequence.last),
                        Some(seed),
                        None,
                        None
                    );
                }
            }
        };
        let rows = prepare_current_module_install_rows_v1(
            &seed, selected, outcome, sources, step, read, parent, module, factory, common, live,
        )?;
        // Keep the complete draft decision until original apply preconditions and
        // certificate verification succeed; a deferred tail business error must not
        // precede their historical first refusal. No further TX reads follow cert.

        let proposal={let mut ctx=overlay!(step,read);crate::world::incremental_normal_action_factory_schedule::with_module_lifecycle_read_io_v1(sources.families.roots,&mut ctx,parent,factory,common,module,|ctx,parent,_,module|{let guard=ctx.transaction.guard();Ok(oasis7_world_state::incremental_runtime_proposal::read_current_runtime_proposal_v1(sequence.proposal_id,sources.proposals,sources.families.roots,parent.state,ctx,module.proposal_scan,module.proposal_read,&guard,live).map_err(overlay_error)?)})}.map_err(source_error)?;
        let prefix = {
            let mut ctx = overlay!(step, read);
            prepare_current_module_apply_prefix_v1(
                proposal, sources, &mut ctx, parent, factory, module, common, directory, now, live,
            )
        }
        .map_err(source_error)?;
        let prefix = match prefix {
            ModuleApplyPrefixDecisionV1::Ready(p) => p,
            rejected => {
                let local = match certificate {
                    InstallCertificateV1::Local(local) => Some(local),
                    InstallCertificateV1::Explicit(_) => None,
                };
                rejected!(
                    CurrentInstallFailureCauseV1::Prefix(rejected),
                    None,
                    Some(sequence.first),
                    Some(sequence.last),
                    Some(seed),
                    keep_ready_rows(rows),
                    local
                );
            }
        };
        let validation = {
            let mut ctx = overlay!(step, read);
            match certificate {
                InstallCertificateV1::Explicit(certificate) => validate_current_module_apply_v1(
                    prefix,
                    certificate,
                    sources,
                    &mut ctx,
                    parent,
                    factory,
                    module,
                    common,
                    now,
                    live,
                ),
                InstallCertificateV1::Local(local) => validate_current_local_module_apply_v1(
                    prefix, local, sources, &mut ctx, parent, factory, module, common, now, live,
                ),
            }
        }?;
        let validated = match validation {
            CurrentModuleApplyValidationV1::Ready(v) => v,
            rejected => {
                rejected!(
                    CurrentInstallFailureCauseV1::Certificate(rejected),
                    None,
                    Some(sequence.first),
                    Some(sequence.last),
                    Some(seed),
                    keep_ready_rows(rows),
                    None
                );
            }
        };
        let rows = match rows {
            CurrentModuleInstallRowsDecisionV1::Ready(rows) => rows,
            rejected => {
                drop(validated);
                rejected!(
                    CurrentInstallFailureCauseV1::Rows(rejected),
                    None,
                    Some(sequence.first),
                    Some(sequence.last),
                    Some(seed),
                    None,
                    None
                );
            }
        };
        let plan = bind_current_install_tail_v1(
            seed,
            rows,
            CurrentInstallGovernanceSourceV1::Apply(validated),
            selected,
            outcome,
            step.transaction,
            common,
            now,
            live,
        )?;
        Ok(ExecutedCurrentSelectedInstallV1::Ready {
            plan,
            accepted: None,
            first: Some(sequence.first),
            last: Some(sequence.last),
        })
    }));
    let result = match attempt {
        Ok(r) => r,
        Err(panic) => {
            poison(step.transaction);
            std::panic::resume_unwind(panic)
        }
    };
    if result.is_err() {
        poison(step.transaction);
    }
    result
}
fn keep_ready_rows(
    rows: CurrentModuleInstallRowsDecisionV1,
) -> Option<PreparedCurrentModuleInstallRowsV1> {
    match rows {
        CurrentModuleInstallRowsDecisionV1::Ready(rows) => Some(rows),
        CurrentModuleInstallRowsDecisionV1::Rejected { .. } => None,
    }
}
fn poison<M: MerkleNodeReader>(tx: &mut ExecutionTransaction<'_, M>) {
    let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> = tx
        .contribute(|_| {
            Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
        });
}
fn source_error(
    error: crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1,
) -> BodyMutationError {
    match error{crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1::Body(e)=>e,crate::world::incremental_normal_action_factory_schedule::FactoryScheduleReadErrorV1::World(_)=>invalid()}
}

/// Runtime-handled install Actions always finish through the historical
/// PostAction(None) lane. Only the final event is routed by the outer handler.
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn execute_install_after_accepted_v1<M, P, V, W, D, E, C, PE, PC, CW, ME, F>(
    selected: &SelectedPendingActionV1,
    outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
    accepted: SelectedAcceptedDispositionV1,
    sources: &ModuleLifecycleSourcesV1<'_>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    parent: &mut WorldFieldReadContextV1<'_, P, V, PE, PC>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    materials: &mut CorePolicyMaterialContextV1<'_, M, ME>,
    module: &mut ModuleLifecycleWorkV1<'_>,
    factory: &mut FactoryScheduleWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
    max_event_bytes: usize,
    persist: &mut F,
    live: &ExecutionFence,
) -> Result<super::event::CompletedModuleLifecycleActionV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    D: FnMut() -> bool,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    PE: FnMut() -> Option<ValueTreeError>,
    PC: FnMut() -> bool,
    CW: FnMut() -> bool,
    ME: FnMut() -> Option<ValueTreeError>,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let result = execute_current_selected_install_v1(
            selected,
            outcome,
            accepted,
            sources,
            step,
            read,
            parent,
            write,
            materials,
            module,
            factory,
            common,
            directory,
            max_event_bytes,
            persist,
            live,
        )?;
        let (first, final_event) = match result {
            ExecutedCurrentSelectedInstallV1::Ready {
                plan,
                accepted,
                first,
                last,
            } => {
                let installed = super::instance_tail_commit::commit_current_module_install_plan_v1(
                    plan,
                    accepted,
                    selected,
                    outcome,
                    sources,
                    step,
                    read,
                    parent,
                    write,
                    materials,
                    module,
                    factory,
                    common,
                    max_event_bytes,
                    persist,
                    live,
                )?;
                drop(last);
                (first, installed)
            }
            ExecutedCurrentSelectedInstallV1::Rejected(failure) => {
                let mut failure = failure.into_business_v1()?;
                let first = failure.first.take();
                let event = super::event::publish_current_module_install_failure_v1(
                    failure,
                    selected,
                    outcome,
                    step,
                    read,
                    materials,
                    common,
                    max_event_bytes,
                    persist,
                    live,
                )?;
                (first, event)
            }
        };
        Ok(match first {
            Some(first) => super::event::CompletedModuleLifecycleActionV1::Applied {
                first,
                second: Some(final_event),
            },
            None => super::event::CompletedModuleLifecycleActionV1::Applied {
                first: final_event,
                second: None,
            },
        })
    }));
    let result = match attempt {
        Ok(r) => r,
        Err(panic) => {
            poison(step.transaction);
            std::panic::resume_unwind(panic)
        }
    };
    if result.is_err() {
        poison(step.transaction);
    }
    result
}
