//! Common non-movement Rule settlement. Movement keeps its business-preflight
//! cost choice in the Core plan; it cannot enter this owner.
use super::*;
use crate::world::{
    incremental_action_admission::UnboundActionAdmissionV1,
    incremental_action_execution::SelectedPendingActionV1,
    incremental_module_invocation::RootRulePhaseOutcomeV1,
    incremental_rule_resources::{
        RuleResourceError, RuleResourceOutcome, settle_current_resources,
    },
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryReservation};

#[derive(Debug)]
pub(in crate::world) enum OrdinarySettlementErrorV1 {
    Body(BodyMutationError),
    Resources(RuleResourceError),
}
#[derive(Debug)]
pub(in crate::world) enum OrdinaryDriverErrorV1<OE> {
    Module(RootModuleExecutionErrorV1<OE>),
    Settlement(OrdinarySettlementErrorV1),
}
impl<OE> From<RootModuleExecutionErrorV1<OE>> for OrdinaryDriverErrorV1<OE> {
    fn from(error: RootModuleExecutionErrorV1<OE>) -> Self {
        Self::Module(error)
    }
}
/// Private evidence of actual common settlement, bound to the exact selected
/// original and immutable Rule outcome. It cannot mint execution permission.
pub(super) struct OrdinarySettlementV1<'selected, 'rule> {
    selected: &'selected SelectedPendingActionV1,
    outcome: &'rule ReservedValue<RootRulePhaseOutcomeV1>,
    resources: Option<ReservedValue<RuleResourceOutcome>>,
    rejection: Option<crate::RejectReason>,
    identity: oasis7_formal_protocol::incremental_execution::PreparedGuard,
    memory: MemoryBudget,
    _reason_charge: MemoryReservation,
}
impl OrdinarySettlementV1<'_, '_> {
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        selected: &SelectedPendingActionV1,
        outcome: &ReservedValue<RootRulePhaseOutcomeV1>,
        tx: &oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        tx.validate_guard(&self.identity, live)?;
        selected.validate(tx)?;
        if !std::ptr::eq(self.selected, selected)
            || !std::ptr::eq(self.outcome, outcome)
            || !self.memory.same_pool(memory)
            || !tx.uses_budget(memory)
        {
            return Err(BodyMutationError::Invalid(
                "ordinary settlement selected/rule/pool",
            ));
        }
        Ok(())
    }
    pub(super) fn applied(&self) -> bool {
        matches!(
            self.resources.as_deref(),
            Some(RuleResourceOutcome::Applied)
        )
    }
    pub(super) fn take_rejection(&mut self) -> Option<crate::RejectReason> {
        self.rejection.take()
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn settle_ordinary_rules_v1<'selected, 'rule, M, R, E, C, W, CW>(
    selected: &'selected SelectedPendingActionV1,
    admission: &UnboundActionAdmissionV1<'_>,
    outcome: &'rule ReservedValue<RootRulePhaseOutcomeV1>,
    read: &mut oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1<
        '_,
        '_,
        '_,
        M,
        R,
        E,
        C,
    >,
    write: &mut oasis7_world_state::incremental_field_member::MemberRecordWriteContext<'_, W, CW>,
    live_owner: &mut dyn FnMut() -> ExecutionFence,
) -> Result<OrdinarySettlementV1<'selected, 'rule>, OrdinarySettlementErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
{
    let result = (|| {
        let body_error = OrdinarySettlementErrorV1::Body;
        admission
            .validate(
                selected,
                read.transaction,
                read.values.memory,
                &live_owner(),
            )
            .map_err(|error| body_error(error.into()))?;
        let effective = outcome
            .override_action_for_selected_v1(selected, read.transaction)
            .map_err(body_error)?
            .unwrap_or(&selected.envelope().action);
        if matches!(effective, crate::Action::MoveAgent { .. })
            || read.values.class != MemoryClass::Ordinary
            || !outcome.uses_budget(read.values.memory)
            || !write.memory.same_pool(read.values.memory)
        {
            return Err(body_error(BodyMutationError::Invalid(
                "nonmovement settlement context/kind",
            )));
        }
        let notes_bytes = if outcome.decision.verdict == crate::RuleVerdict::Deny {
            outcome
                .decision
                .notes
                .iter()
                .try_fold(1024usize, |sum, note| {
                    sum.checked_add(std::mem::size_of::<String>())
                        .and_then(|sum| sum.checked_add(note.len()))
                        .ok_or(BodyMutationError::Invalid("ordinary denial notes bound"))
                })
                .map_err(body_error)?
        } else {
            1024
        };
        let fee = read
            .values
            .memory
            .reserve(notes_bytes, MemoryClass::Ordinary)
            .map_err(|error| body_error(ValueTreeError::Memory(error).into()))?;
        if (read.cancelled)() || (write.cancelled)() {
            return Err(body_error(ValueTreeError::Cancelled.into()));
        }
        let mut rejection = if outcome.decision.verdict == crate::RuleVerdict::Deny {
            Some(crate::RejectReason::RuleDenied {
                notes: outcome.decision.notes.clone(),
            })
        } else {
            None
        };
        let resources = if rejection.is_none() {
            Some(
                settle_current_resources(&outcome.decision.cost, read, write, &live_owner())
                    .map_err(OrdinarySettlementErrorV1::Resources)?,
            )
        } else {
            None
        };
        match resources.as_deref() {
            Some(RuleResourceOutcome::Applied) | None => {}
            Some(RuleResourceOutcome::Insufficient(deficits)) => {
                rejection = Some(crate::RejectReason::InsufficientResources {
                    deficits: deficits.iter().copied().collect(),
                });
            }
            Some(RuleResourceOutcome::Overflow(error)) => {
                rejection = Some(crate::RejectReason::RuleDenied {
                    notes: vec![format!("rule decision cost apply rejected: {error:?}")],
                });
            }
        }
        if (read.cancelled)() || (write.cancelled)() {
            return Err(body_error(ValueTreeError::Cancelled.into()));
        }
        read.transaction
            .validate_guard(&read.transaction.guard(), &live_owner())
            .map_err(|error| body_error(error.into()))?;
        Ok(OrdinarySettlementV1 {
            selected,
            outcome,
            resources,
            rejection,
            identity: read.transaction.guard(),
            memory: read.values.memory.clone(),
            _reason_charge: fee,
        })
    })();
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            read.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}
