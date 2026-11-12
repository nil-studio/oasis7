//! Core preflight is separate from settlement and event publication.
use super::*;
use crate::world::{
    incremental_action_execution::SelectedPendingActionV1,
    incremental_module_invocation::RootRulePhaseOutcomeV1,
};
use oasis7_formal_protocol::incremental_execution::{ExecutionTransaction, PreparedGuard};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryReservation};

/// The exact immutable Rule owner stays borrowed through settlement. A plan
/// cannot be paired with another evaluation's cost, cloned or published twice.
/// Its body owns only this action's bounded result, never a materialized World.
pub(in crate::world) struct PreparedCoreActionV1<'rule> {
    body: crate::DomainEvent,
    _charge: MemoryReservation,
    outcome: &'rule ReservedValue<RootRulePhaseOutcomeV1>,
    identity: PreparedGuard,
    action_id: u64,
    time: u64,
    movement: bool,
}
impl PreparedCoreActionV1<'_> {
    fn validate<M: MerkleNodeReader>(
        &self,
        selected: &SelectedPendingActionV1,
        transaction: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
    ) -> Result<(), BodyMutationError> {
        transaction.validate_transaction_identity(&self.identity)?;
        selected.validate(transaction)?;
        if !transaction.uses_budget(memory)
            || !self.outcome.uses_budget(memory)
            || self.action_id != selected.envelope().id
        {
            return Err(BodyMutationError::Invalid(
                "Core plan selected/pool binding",
            ));
        }
        self.outcome
            .override_action_for_selected_v1(selected, transaction)?;
        Ok(())
    }

    /// World::step chooses the original general-rule cost when the final Move
    /// is rejected by business preflight. Pricing is not charged for distance
    /// that was never authorized. A derived Move from another kind has no
    /// movement-specific alternative, preserving the original fallback.
    pub(in crate::world) fn cost<'a, M: MerkleNodeReader>(
        &'a self,
        selected: &SelectedPendingActionV1,
        transaction: &mut ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
    ) -> Result<&'a crate::ResourceDelta, BodyMutationError> {
        transaction.contribute(|transaction| {
            self.validate(selected, transaction, memory)?;
            Ok(
                if self.movement && matches!(&self.body, crate::DomainEvent::ActionRejected { .. })
                {
                    self.outcome
                        .movement_rejection_cost
                        .as_ref()
                        .unwrap_or(&self.outcome.decision.cost)
                } else {
                    &self.outcome.decision.cost
                },
            )
        })
    }
}

/// Prepare before global resource settlement. No business mutation, event
/// original, allocator advance or Event fact is contributed here.
#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_selected_core_action_v1<'rule, M, P, V, W, E, C, D>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    selected: &SelectedPendingActionV1,
    admission: &crate::world::incremental_action_admission::UnboundActionAdmissionV1<'_>,
    outcome: &'rule ReservedValue<RootRulePhaseOutcomeV1>,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    policy_scan: &mut StateScanBudget,
    policy_work: &mut TypedRecordReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<PreparedCoreActionV1<'rule>, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    D: FnMut() -> bool,
{
    prepare_scoped_selected_core_action_v1(
        step,
        selected,
        crate::world::incremental_action_admission::CoreActionAdmissionV1::Unbound(admission),
        None,
        outcome,
        agents,
        read,
        policy_scan,
        policy_work,
        guard,
        live,
    )
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn prepare_scoped_selected_core_action_v1<'rule, M, P, V, W, E, C, D>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    selected: &SelectedPendingActionV1,
    admission: crate::world::incremental_action_admission::CoreActionAdmissionV1<'_, '_, '_>,
    common: Option<&mut oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1<'_>>,
    outcome: &'rule ReservedValue<RootRulePhaseOutcomeV1>,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    policy_scan: &mut StateScanBudget,
    policy_work: &mut TypedRecordReadWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<PreparedCoreActionV1<'rule>, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    D: FnMut() -> bool,
{
    let result = (|| {
        step.transaction.validate_guard(guard, live)?;
        selected.validate(step.transaction)?;
        admission.validate(
            selected,
            step.transaction,
            step.mutation.memory,
            common,
            live,
        )?;
        step.journal_height()?;
        let memory = step.mutation.memory;
        if !step.transaction.uses_budget(memory)
            || !outcome.uses_budget(memory)
            || !agents.uses_budget(memory)
            || !read.values.memory.same_pool(memory)
            || read.values.class != MemoryClass::Ordinary
        {
            return Err(BodyMutationError::Invalid(
                "selected core action pool/class",
            ));
        }
        read.check().map_err(BodyMutationError::Value)?;
        let action = outcome
            .override_action_for_selected_v1(selected, step.transaction)?
            .unwrap_or(&selected.envelope().action);
        let agent = match action {
            crate::Action::RegisterAgent { agent_id, .. }
            | crate::Action::MoveAgent { agent_id, .. } => agent_id,
            _ => {
                return Err(BodyMutationError::Invalid(
                    "selected core action kind unsupported",
                ));
            }
        };
        let notes_bytes = if outcome.decision.verdict == crate::RuleVerdict::Deny {
            outcome
                .decision
                .notes
                .iter()
                .try_fold(0usize, |sum, note| {
                    sum.checked_add(std::mem::size_of::<String>())
                        .and_then(|sum| sum.checked_add(note.len()))
                        .ok_or(BodyMutationError::Invalid(
                            "Core rule rejection notes bound",
                        ))
                })?
        } else {
            0
        };
        let bytes = agent
            .len()
            .checked_mul(8)
            .and_then(|n| n.checked_add(16 << 10))
            .and_then(|n| n.checked_add(notes_bytes))
            .ok_or(BodyMutationError::Invalid(
                "selected core action scratch bound",
            ))?;
        let charge = memory
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(|error| BodyMutationError::Value(ValueTreeError::Memory(error)))?;
        if (step.mutation.cancelled)() || (read.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let body = if outcome.decision.verdict == crate::RuleVerdict::Deny {
            crate::DomainEvent::ActionRejected {
                action_id: selected.envelope().id,
                reason: crate::RejectReason::RuleDenied {
                    notes: outcome.decision.notes.clone(),
                },
            }
        } else {
            prepare_core_body_v1(
                step,
                selected,
                action,
                agents,
                read,
                policy_scan,
                policy_work,
                live,
            )?
        };
        read.check().map_err(BodyMutationError::Value)?;
        if (step.mutation.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(PreparedCoreActionV1 {
            body,
            _charge: charge,
            outcome,
            identity: step.transaction.guard(),
            action_id: selected.envelope().id,
            time: step.time(),
            movement: matches!(action, crate::Action::MoveAgent { .. }),
        })
    })();
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            step.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}

/// Consume the prepared body only after the common driver settles its selected
/// cost. The caller supplies a fresh guard after those same-transaction writes.
/// Failure poisons settlement and all preceding contributions together.
#[allow(clippy::too_many_arguments)]
fn apply_prepared_core_action_v1<M, P, V, W, E, C, D, F>(
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, D>,
    materials: &mut super::super::incremental_body_event::materials::CorePolicyMaterialContextV1<
        '_,
        M,
        impl FnMut() -> Option<ValueTreeError>,
    >,
    selected: &SelectedPendingActionV1,
    prepared: PreparedCoreActionV1<'_>,
    agents: &ReservedValue<AgentSubtreeStructureQualificationV1>,
    read: &mut WorldFieldReadContextV1<'_, P, V, E, C>,
    audit_work: &mut oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1,
    max_event_bytes: usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
    persist: F,
) -> Result<CompletedCoreActionV1, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    D: FnMut() -> bool,
    F: FnMut(
        &mut dyn super::super::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
{
    let result = (|| {
        let memory = step.mutation.memory;
        step.transaction.validate_guard(guard, live)?;
        prepared.validate(selected, step.transaction, memory)?;
        step.journal_height()?;
        if prepared.time != step.time()
            || !agents.uses_budget(memory)
            || !read.values.memory.same_pool(memory)
            || read.values.class != MemoryClass::Ordinary
        {
            return Err(BodyMutationError::Invalid("Core plan time/current binding"));
        }
        read.check().map_err(BodyMutationError::Value)?;
        if (step.mutation.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let PreparedCoreActionV1 {
            body,
            _charge: _phase,
            ..
        } = prepared;
        let event = publish_core_body_v1(
            step,
            materials,
            selected,
            body,
            agents,
            read,
            max_event_bytes,
            live,
            persist,
        )?;
        let event = oasis7_world_state::incremental_world_event_owned::retain_world_event_owned_v1(
            event,
            memory,
            MemoryClass::Ordinary,
            audit_work,
            &mut step.mutation.cancelled,
        )
        .map_err(BodyMutationError::Value)?;
        read.check().map_err(BodyMutationError::Value)?;
        if (step.mutation.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        step.transaction
            .validate_guard(&step.transaction.guard(), live)?;
        Ok(CompletedCoreActionV1 {
            event_before_post_action: false,
            event,
            identity: step.transaction.guard(),
        })
    })();
    if result.is_err() {
        let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
            step.transaction.contribute(|_| {
                Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
            });
    }
    result
}

#[path = "incremental_action_execution_core_settlement.rs"]
mod settlement;
pub(in crate::world) use settlement::{
    CoreSettlementErrorV1, apply_settled_core_action_v1, settle_prepared_core_action_v1,
};
