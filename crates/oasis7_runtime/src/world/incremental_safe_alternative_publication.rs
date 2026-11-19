//! Fixed publication handoff after actual result projection and exact CAS.
//! Nested under finalization so no caller can manufacture the event input.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1;
use oasis7_proto::memory_budget::MemoryClass;

pub(in crate::world) struct PreparedSafeAlternativeOutcomeRecordedV1 {
    event: crate::CapabilityAuthorizationEvent,
    receipt: ReservedValue<SafeAlternativeRecordWriteReceiptV1>,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    _fee: ReservedValue<()>,
}
impl PreparedSafeAlternativeOutcomeRecordedV1 {
    pub(in crate::world) fn receipt(&self) -> &SafeAlternativeRecordWriteReceiptV1 {
        &self.receipt
    }
    pub(in crate::world) fn authorization_event(&self) -> &crate::CapabilityAuthorizationEvent {
        &self.event
    }
    pub(in crate::world) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        tx.validate_guard(&self.guard, live)?;
        self.receipt
            .validate_transaction(tx)
            .map_err(overlay_error)?;
        if self.core != common.work().identity() || !self._fee.uses_budget(common.memory()) {
            return Err(BodyMutationError::Invalid(
                "SafeAlternative OutcomeRecorded publication binding",
            ));
        }
        Ok(())
    }
}

impl AppliedSafeAlternativeOutcomeV1<'_, '_> {
    /// Release the signed source/ledger borrows only after transferring the
    /// exact fixed event into a paid private owner. The next fixed publisher can
    /// now update the same mutable side ledger; no parent root is substituted.
    pub(in crate::world) fn prepare_publication<M: MerkleNodeReader>(
        self,
        tx: &mut ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<PreparedSafeAlternativeOutcomeRecordedV1, BodyMutationError> {
        let result = (|| {
            self.validate(tx, common, live)?;
            let bytes = std::mem::size_of::<PreparedSafeAlternativeOutcomeRecordedV1>()
                .checked_add(self.key().len())
                .ok_or(ValueTreeError::ResourceLimited(
                    "SafeAlternative publication layout",
                ))?;
            let fee = common
                .memory()
                .reserve(bytes, MemoryClass::Ordinary)
                .map_err(ValueTreeError::Memory)?;
            common
                .charge(4, self.key().len() as u64)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            let mut key = String::with_capacity(self.key().len());
            if key.capacity() > self.key().len() {
                return Err(ValueTreeError::ResourceLimited(
                    "SafeAlternative publication key capacity",
                )
                .into());
            }
            key.push_str(self.key());
            let event = crate::CapabilityAuthorizationEvent::SafeAlternativeOutcomeRecorded {
                key,
                action_id: self.action_id(),
                event_id: self.result_event_id(),
            };
            common
                .consume_steps(0)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            Ok(PreparedSafeAlternativeOutcomeRecordedV1 {
                event,
                receipt: self.receipt,
                guard: tx.guard(),
                core: common.work().identity(),
                _fee: fee.retain(()),
            })
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> = tx
                .contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }
}
