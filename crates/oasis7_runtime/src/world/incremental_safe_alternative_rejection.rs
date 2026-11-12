//! Genuine fixed guard rejection publication. Nested under matching: callers
//! cannot supply a reason or manufacture the private input owner.
use super::*;
use crate::world::{
    incremental_body_event::{AgentEventOriginalV1, materials::CorePolicyMaterialContextV1},
    incremental_step::IwsStepContext,
};
use oasis7_formal_protocol::incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError};
use oasis7_proto::memory_budget::MemoryClass;

/// Owns the matching input until finalization; the event is retained with its
/// real stream/counter/allocator receipt, never reconstructed from caller DATA.
pub(in crate::world) struct CompletedSafeAlternativeRejectionV1<'selected, 'source> {
    pub(super) input: PaidSignedSafeAlternativeRecordV1<'selected, 'source>,
    event: ReservedValue<crate::WorldEvent>,
    identity: PreparedGuard,
    _scope_fee: ReservedValue<()>,
    _outer: ReservedValue<()>,
}
impl CompletedSafeAlternativeRejectionV1<'_, '_> {
    pub(in crate::world) fn event(&self) -> &crate::WorldEvent {
        &self.event
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
    ) -> Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> {
        tx.validate_transaction_identity(&self.identity)
    }
}

/// This runs after the real Accepted publication. Only a rejection permission
/// is used after that revision: it cannot perform a Core business mutation.
/// Exact Current member CAS is still mandatory during finalization.
pub(in crate::world) struct PreparedSafeAlternativeRejectionV1<'selected, 'source> {
    input: PaidSignedSafeAlternativeRecordV1<'selected, 'source>,
    reason: String,
    identity: PreparedGuard,
    _scope_fee: ReservedValue<()>,
    _outer: ReservedValue<()>,
}
impl<'selected, 'source> PaidRejectedSafeAlternativeV1<'selected, 'source> {
    pub(in crate::world) fn prepare_rejection<M: MerkleNodeReader>(
        self,
        selected: &SelectedPendingActionV1,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<PreparedSafeAlternativeRejectionV1<'selected, 'source>, BodyMutationError> {
        self.validate(selected, tx, common, live)?;
        Ok(PreparedSafeAlternativeRejectionV1 {
            input: self.input,
            reason: self.reason,
            identity: self.guard,
            _scope_fee: self._scope_fee,
            _outer: self._outer,
        })
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::world) fn publish_safe_alternative_rejection_v1<'selected, 'source, M, V, W, C, F>(
    prepared: PreparedSafeAlternativeRejectionV1<'selected, 'source>,
    step: &mut IwsStepContext<'_, '_, '_, '_, M, V, W, C>,
    selected: &SelectedPendingActionV1,
    materials: &mut CorePolicyMaterialContextV1<'_, M, impl FnMut() -> Option<ValueTreeError>>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
    max_event_bytes: usize,
    persist: F,
) -> Result<CompletedSafeAlternativeRejectionV1<'selected, 'source>, BodyMutationError>
where
    M: MerkleNodeReader,
    V: ValueNodeReader,
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let result = (|| {
        selected.validate(step.transaction)?;
        step.transaction
            .validate_transaction_identity(&prepared.identity)?;
        if !std::ptr::eq(prepared.input.selected, selected)
            || !prepared._outer.uses_budget(common.memory())
            || !prepared._scope_fee.uses_budget(common.memory())
            || !step.transaction.uses_budget(common.memory())
        {
            return Err(BodyMutationError::Invalid(
                "SafeAlternative rejection publication source",
            ));
        }
        // Move the already charged reason into the original one-note Vec.
        // A separate retained event reservation covers the moved reason too;
        // source phase fees remain live until the completed receipt drops.
        let bytes = std::mem::size_of::<crate::WorldEvent>()
            .checked_add(4 * std::mem::size_of::<String>())
            .and_then(|n| n.checked_add(prepared.reason.len()))
            .ok_or(ValueTreeError::ResourceLimited(
                "SafeAlternative rejection event layout",
            ))?;
        let fee = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        common
            .charge(8, prepared.reason.len() as u64)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        let event = step.append_selected_rejection_event(
            materials,
            live,
            selected,
            crate::RejectReason::RuleDenied {
                notes: vec![prepared.reason],
            },
            max_event_bytes,
            persist,
        )?;
        common
            .consume_steps(0)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        Ok(CompletedSafeAlternativeRejectionV1 {
            input: prepared.input,
            event: fee.retain(event),
            identity: step.transaction.guard(),
            _scope_fee: prepared._scope_fee,
            _outer: prepared._outer,
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
