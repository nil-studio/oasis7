//! Core business execution in the actual Root module context. Queue selection,
//! common admission and Rule evaluation remain genuine upstream owners.
use super::*;
use crate::world::{
    incremental_action_admission::UnboundActionAdmissionV1,
    incremental_action_execution::SelectedPendingActionV1,
    incremental_action_execution_core::{
        CompletedCoreActionV1, apply_settled_core_action_v1, settle_prepared_core_action_v1,
    },
    incremental_module_execution_event::PersistedWorldEventV1,
    incremental_module_invocation::RootRulePhaseOutcomeV1,
    incremental_step::IwsStepContext,
};

pub(in crate::world) enum CoreDriverErrorV1<OE> {
    Module(RootModuleExecutionErrorV1<OE>),
    Settlement(crate::world::incremental_action_execution_core::CoreSettlementErrorV1),
    SelectedRules(super::selected_rule::SelectedRuleLaneErrorV1<OE>),
}
impl<OE: std::fmt::Debug> std::fmt::Debug for CoreDriverErrorV1<OE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Module(error) => f.debug_tuple("Module").field(error).finish(),
            Self::Settlement(error) => f.debug_tuple("Settlement").field(error).finish(),
            Self::SelectedRules(error) => f.debug_tuple("SelectedRules").field(error).finish(),
        }
    }
}
impl<OE> From<RootModuleExecutionErrorV1<OE>> for CoreDriverErrorV1<OE> {
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
    /// Run the common selected Core sequence in this Root context. The Current
    /// owner is supplied by the transaction owner and maintained by its deltas;
    /// this entry never reconstructs a World or scans all alternatives per input.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn execute_selected_core_v1<P, ER, CR, AP, AE, AC>(
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
        materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<'_, M, impl FnMut() -> Option<ValueTreeError>>,
        read: &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<
            '_,
            P,
            R,
            ER,
            CR,
        >,
        policy_scan: &mut StateScanBudget,
        policy_work: &mut oasis7_world_state::incremental_runtime_revocation::RevocationReadWorkV1,
        audit_work: &mut oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        rule_work: &mut crate::world::incremental_module_invocation::RootRulePhaseWorkV1,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        tail_work: &mut crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<
        (ReservedValue<RootRulePhaseOutcomeV1>, CompletedCoreActionV1),
        CoreDriverErrorV1<O::Error>,
    >
    where
        P: MerkleNodeReader,
        ER: FnMut() -> Option<ValueTreeError>,
        CR: FnMut() -> bool,
        AP: MerkleNodeReader,
        AE: FnMut() -> Option<ValueTreeError>,
        AC: FnMut() -> bool,
    {
        let result = (|| {
            if !matches!(
                selected.envelope().action,
                crate::Action::RegisterAgent { .. } | crate::Action::MoveAgent { .. }
            ) {
                return Err(CoreDriverErrorV1::Module(RootModuleExecutionErrorV1::Body(
                    BodyMutationError::Invalid("Core driver original action family"),
                )));
            }
            let (outcome, admission) = self
                .prepare_selected_rule_lane_v1(
                    selected,
                    qualification,
                    current,
                    admission_read,
                    policy_scan,
                    policy_work,
                    audit_work,
                    directory,
                    rule_work,
                    live_owner,
                )
                .map_err(CoreDriverErrorV1::SelectedRules)?;
            let completed = self.execute_core_after_rules_v1::<P, ER, CR, ER, CR>(
                selected,
                &admission,
                &outcome,
                materials,
                read,
                None,
                None,
                policy_scan,
                policy_work,
                audit_work,
                directory,
                enumeration,
                tail_work,
                live_owner,
            )?;
            Ok((outcome, completed))
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }

    /// Consume a genuine Core decision, settle Current resources, publish
    /// through the original owner, then run the original two-stage routing order.
    /// No completed tick, Accepted parent or activation authority is minted.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn execute_core_after_rules_v1<P, ER, CR, PE, PC>(
        &mut self,
        selected: &SelectedPendingActionV1,
        admission: &UnboundActionAdmissionV1<'_>,
        outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
        materials: &mut crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1<
            '_, M, impl FnMut() -> Option<ValueTreeError>>,
        read: &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<
            '_,
            P,
            R,
            ER,
            CR,
        >,
        parent_read: Option<
            &mut oasis7_world_state::incremental_field_read::WorldFieldReadContextV1<
                '_,
                P,
                R,
                PE,
                PC,
            >,
        >,
        accepted_entries: Option<&mut usize>,
        policy_scan: &mut StateScanBudget,
        policy_work: &mut oasis7_world_state::incremental_runtime_revocation::RevocationReadWorkV1,
        audit_work: &mut oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        tail_work: &mut crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<CompletedCoreActionV1, CoreDriverErrorV1<O::Error>>
    where
        P: MerkleNodeReader,
        ER: FnMut() -> Option<ValueTreeError>,
        CR: FnMut() -> bool,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    {
        let result = (|| {
            let completed = self.execute_scoped_core_before_post_v1::<P, ER, CR, PE, PC>(
                selected,
                crate::world::incremental_action_admission::CoreActionAdmissionV1::Unbound(
                    admission,
                ),
                None,
                outcome,
                materials,
                read,
                parent_read,
                accepted_entries,
                policy_scan,
                policy_work,
                audit_work,
                directory,
                enumeration,
                live_owner,
            )?;
            self.finish_core_post_action_v1(
                selected,
                outcome,
                &completed,
                directory,
                enumeration,
                tail_work,
                live_owner,
            )?;
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

#[path = "incremental_module_execution_core_before_post.rs"]
mod before_post;
