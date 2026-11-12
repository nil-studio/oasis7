//! Sealed execution permission. DATA proofs never expose a constructor.
//! The sole actual-Current scope builder must finish every original predicate
//! before constructing this type; storage readers cannot mint it.
use super::{
    incremental_action_execution::SelectedPendingActionV1, incremental_body::BodyMutationError,
    incremental_safe_alternative_record::PaidSignedSafeAlternativeRecordV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::MerkleNodeReader,
};
use oasis7_proto::memory_budget::{MemoryBudget, ReservedValue};
pub(super) struct PaidMatchingSafeAlternativeV1<'selected, 'source> {
    input: PaidSignedSafeAlternativeRecordV1<'selected, 'source>,
    guard: PreparedGuard,
    _outer: ReservedValue<()>,
}
impl PaidMatchingSafeAlternativeV1<'_, '_> {
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        selected: &SelectedPendingActionV1,
        tx: &ExecutionTransaction<'_, M>,
        memory: &MemoryBudget,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.input.validate_before_scope(tx, common, live)?;
        tx.validate_guard(&self.guard, live)?;
        if !std::ptr::eq(self.input.selected, selected)
            || !self._outer.uses_budget(memory)
            || !common.memory().same_pool(memory)
        {
            return Err(BodyMutationError::Invalid(
                "matching SafeAlternative scope binding",
            ));
        }
        Ok(())
    }
}
