//! Actual selected admission -> economy worker -> Rule ordering. Rejection is
//! an explicit no-Rule branch and cannot manufacture a default Rule decision.
use super::selected_rule::SelectedRuleLaneErrorV1;
use super::*;
use crate::world::{
    incremental_action_admission::{
        SelectedActionAdmissionV1, UnboundActionAdmissionV1, admit_selected_action_v1,
    },
    incremental_action_execution::SelectedPendingActionV1,
    incremental_normal_action_factory_schedule::{FactoryScheduleSourcesV1, FactoryScheduleWorkV1},
};
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1;

pub(in crate::world) enum PreparedSelectedFactoryRuleLaneV1<'selected> {
    Rules {
        outcome: ReservedValue<RootRulePhaseOutcomeV1>,
        admission: ReservedValue<UnboundActionAdmissionV1<'selected>>,
    },
    Rejected {
        outcome: FactoryModuleBeforeRulesOutcomeV1<'selected>,
        initial: factory_module_rejection::FactoryModuleInitialAdmissionV1<'selected>,
    },
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
    fn current_unbound_rule_admission_v1<'selected, AP, AE, AC>(
        &mut self,
        selected: &'selected SelectedPendingActionV1,
        qualification: &ReservedValue<
            oasis7_world_state::incremental_runtime_revocation::RuntimeRevocationQualificationV1,
        >,
        current: &oasis7_world_state::incremental_runtime_revocation::RuntimeRevocationCurrentV1,
        read: &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<
            '_,
            AP,
            R,
            AE,
            AC,
        >,
        scan: &mut StateScanBudget,
        work: &mut oasis7_world_state::incremental_runtime_revocation::RevocationReadWorkV1,
        live: &ExecutionFence,
    ) -> Result<ReservedValue<UnboundActionAdmissionV1<'selected>>, SelectedRuleLaneErrorV1<O::Error>>
    where
        AP: MerkleNodeReader,
        AE: FnMut() -> Option<ValueTreeError>,
        AC: FnMut() -> bool,
    {
        let guard = self.ctx.transaction.guard();
        match admit_selected_action_v1(
            selected,
            qualification,
            current,
            self.ctx,
            read,
            scan,
            work,
            &guard,
            live,
        )
        .map_err(SelectedRuleLaneErrorV1::Admission)?
        {
            SelectedActionAdmissionV1::Unbound(value) => Ok(value),
            SelectedActionAdmissionV1::RequiresSafeAlternativeScope(record) => Err(
                SelectedRuleLaneErrorV1::SafeAlternativeScopeRequired(record),
            ),
        }
    }

    pub(in crate::world) fn prepare_selected_factory_module_rule_lane_v1<
        'selected,
        AP,
        AE,
        AC,
        P,
        PE,
        PC,
    >(
        &mut self,
        selected: &'selected SelectedPendingActionV1,
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
        sources: &FactoryScheduleSourcesV1<'_>,
        parent: &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<
            '_,
            P,
            R,
            PE,
            PC,
        >,
        work: &mut FactoryScheduleWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        audit_work: &mut oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        rule_work: &mut RootRulePhaseWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<PreparedSelectedFactoryRuleLaneV1<'selected>, SelectedRuleLaneErrorV1<O::Error>>
    where
        AP: MerkleNodeReader,
        AE: FnMut() -> Option<ValueTreeError>,
        AC: FnMut() -> bool,
        P: MerkleNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let initial = self.current_unbound_rule_admission_v1(
                selected,
                qualification,
                current,
                admission_read,
                policy_scan,
                policy_work,
                &live_owner(),
            )?;
            let initial = factory_module_rejection::prepare_factory_module_initial_admission_v1(
                initial,
                selected,
                self.ctx.transaction,
                common,
                self.now,
                &live_owner(),
            )
            .map_err(RootModuleExecutionErrorV1::Body)?;
            let resolution = self.resolve_selected_factory_module_before_rules_v1(
                selected, sources, parent, work, common, live_owner,
            )?;
            let resolved = match resolution {
                FactoryModuleBeforeRulesOutcomeV1::Resolved(resolved) => resolved,
                outcome => {
                    return Ok(PreparedSelectedFactoryRuleLaneV1::Rejected { outcome, initial });
                }
            };
            let outcome = self.execute_resolved_factory_rule_phases_v1(
                selected, resolved, common, directory, rule_work, live_owner,
            )?;
            self.publish_action_override_v1(selected, &outcome, audit_work, live_owner)?;
            let admission = self.current_unbound_rule_admission_v1(
                selected,
                qualification,
                current,
                admission_read,
                policy_scan,
                policy_work,
                &live_owner(),
            )?;
            Ok(PreparedSelectedFactoryRuleLaneV1::Rules { outcome, admission })
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
