//! Shared Root admission/Rule/Modify consumer for genuine selected input.
use super::*;
use crate::world::{
    incremental_action_admission::UnboundActionAdmissionV1,
    incremental_action_execution::SelectedPendingActionV1,
    incremental_module_invocation::RootRulePhaseOutcomeV1,
};
pub(in crate::world) enum SelectedRuleLaneErrorV1<OE> {
    Module(RootModuleExecutionErrorV1<OE>),
    Admission(crate::world::incremental_action_admission::ActionAdmissionErrorV1),
    SafeAlternativeScopeRequired(
        ReservedValue<
            oasis7_world_state::incremental_runtime_revocation::CurrentSafeAlternativeRecordV1,
        >,
    ),
}
impl<OE: std::fmt::Debug> std::fmt::Debug for SelectedRuleLaneErrorV1<OE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Module(error) => f.debug_tuple("Module").field(error).finish(),
            Self::Admission(error) => f.debug_tuple("Admission").field(error).finish(),
            Self::SafeAlternativeScopeRequired(_) => f.write_str("SafeAlternativeScopeRequired"),
        }
    }
}
impl<OE> From<RootModuleExecutionErrorV1<OE>> for SelectedRuleLaneErrorV1<OE> {
    fn from(error: RootModuleExecutionErrorV1<OE>) -> Self {
        Self::Module(error)
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
    /// Consume actual admission, Rule and Modify owners in order. This is only
    /// the unprotected/no-matching-alternative lane; business, economy and
    /// settlement remain with the selected family owner.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn prepare_selected_rule_lane_v1<'selected, AP, AE, AC>(
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
        audit_work: &mut oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        rule_work: &mut crate::world::incremental_module_invocation::RootRulePhaseWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<
        (
            ReservedValue<RootRulePhaseOutcomeV1>,
            ReservedValue<UnboundActionAdmissionV1<'selected>>,
        ),
        SelectedRuleLaneErrorV1<O::Error>,
    >
    where
        AP: MerkleNodeReader,
        AE: FnMut() -> Option<ValueTreeError>,
        AC: FnMut() -> bool,
    {
        use crate::world::incremental_action_admission::{
            SelectedActionAdmissionV1, admit_selected_action_v1,
        };
        let result = (|| {
            let live = live_owner();
            let guard = self.ctx.transaction.guard();
            let initial = admit_selected_action_v1(
                selected,
                qualification,
                current,
                self.ctx,
                admission_read,
                policy_scan,
                policy_work,
                &guard,
                &live,
            )
            .map_err(SelectedRuleLaneErrorV1::Admission)?;
            match initial {
                SelectedActionAdmissionV1::RequiresSafeAlternativeScope(record) => {
                    return Err(SelectedRuleLaneErrorV1::SafeAlternativeScopeRequired(
                        record,
                    ));
                }
                SelectedActionAdmissionV1::Unbound(permission) => drop(permission),
            }
            let outcome =
                self.execute_pre_action_rule_phases_v1(selected, directory, rule_work, live_owner)?;
            // The original Modify audit precedes business preflight/settlement.
            // Its producer, not a caller DTO, contributes the counter fact.
            let _override =
                self.publish_action_override_v1(selected, &outcome, audit_work, live_owner)?;
            let live = live_owner();
            let guard = self.ctx.transaction.guard();
            let fresh = admit_selected_action_v1(
                selected,
                qualification,
                current,
                self.ctx,
                admission_read,
                policy_scan,
                policy_work,
                &guard,
                &live,
            )
            .map_err(SelectedRuleLaneErrorV1::Admission)?;
            let admission = match fresh {
                SelectedActionAdmissionV1::Unbound(permission) => permission,
                SelectedActionAdmissionV1::RequiresSafeAlternativeScope(record) => {
                    return Err(SelectedRuleLaneErrorV1::SafeAlternativeScopeRequired(
                        record,
                    ));
                }
            };
            Ok((outcome, admission))
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
