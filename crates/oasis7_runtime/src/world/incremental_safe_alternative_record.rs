//! Signed Current record joined to genuine Selected and independent Current
//! ProofLedger. This is an input proof; business scope remains to be checked.
use super::{
    incremental_action_execution::SelectedPendingActionV1, incremental_body::BodyMutationError,
    incremental_safe_alternative_allocation::safe_alternative_owned_bound_v1,
    incremental_safe_alternative_proof_ledger::PaidSafeAlternativeProofLedgerV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::MerkleNodeReader,
    incremental_value::ValueTreeError,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::incremental_runtime_revocation::CurrentSafeAlternativeRecordV1;
#[derive(Debug)]
struct SafeAlternativeHistoryErrorV1(crate::WorldError);
impl std::fmt::Display for SafeAlternativeHistoryErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl std::error::Error for SafeAlternativeHistoryErrorV1 {}
pub(super) struct PaidSignedSafeAlternativeRecordV1<'a, 'p> {
    pub(super) record: ReservedValue<CurrentSafeAlternativeRecordV1>,
    pub(super) selected: &'a SelectedPendingActionV1,
    pub(super) ledger: &'p PaidSafeAlternativeProofLedgerV1<'p>,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    _phase: ReservedValue<()>,
}
impl PaidSignedSafeAlternativeRecordV1<'_, '_> {
    pub(super) fn validate_before_scope<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        self.selected.validate(tx)?;
        self.ledger.validate(tx, common, live)?;
        tx.validate_guard(&self.guard, live)?;
        if self.core != common.work().identity()
            || !self.record.uses_budget(common.memory())
            || !self._phase.uses_budget(common.memory())
        {
            return Err(BodyMutationError::Invalid(
                "SafeAlternative signed record source",
            ));
        }
        Ok(())
    }
}
pub(super) fn prepare_signed_safe_alternative_record_v1<'a, 'p, M: MerkleNodeReader>(
    record: ReservedValue<CurrentSafeAlternativeRecordV1>,
    selected: &'a SelectedPendingActionV1,
    ledger: &'p PaidSafeAlternativeProofLedgerV1<'p>,
    tx: &mut ExecutionTransaction<'_, M>,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PaidSignedSafeAlternativeRecordV1<'a, 'p>, BodyMutationError> {
    let result = (|| {
        selected.validate(tx)?;
        ledger.validate(tx, common, live)?;
        record.validate(tx, live).map_err(|e| match e {
            oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Execution(
                e,
            ) => BodyMutationError::Execution(e),
            oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Value(e) => {
                BodyMutationError::Value(e)
            }
            oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Member(e) => {
                BodyMutationError::Original(Box::new(e))
            }
        })?;
        if !record.uses_budget(common.memory())
            || record.record()["authorization"]["action_id"].as_u64()
                != Some(selected.envelope().id)
        {
            return Err(BodyMutationError::Invalid(
                "SafeAlternative Selected record",
            ));
        }
        let bytes = safe_alternative_owned_bound_v1(record.record(), common)?
            .checked_add(std::mem::size_of::<PaidSignedSafeAlternativeRecordV1<'a, 'p>>())
            .ok_or(ValueTreeError::ResourceLimited(
                "SafeAlternative signed record layout",
            ))?;
        let phase = common
            .memory()
            .reserve(bytes, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        common
            .charge(bytes as u64, bytes as u64)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        // The complete Current action index already rejected duplicate action IDs.
        // This set preserves the original per-record validation order; it is not a
        // caller assertion of global uniqueness or a replacement for that index.
        let mut seen = std::collections::BTreeSet::new();
        super::incremental_safe_alternative_history::validate_safe_alternative_record_history_v1(
            record.lineage(),
            record.record(),
            Some(selected.envelope()),
            &mut seen,
        )
        .map_err(|e| BodyMutationError::Original(Box::new(SafeAlternativeHistoryErrorV1(e))))?;
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        Ok(PaidSignedSafeAlternativeRecordV1 {
            record,
            selected,
            ledger,
            guard: tx.guard(),
            core: common.work().identity(),
            _phase: phase.retain(()),
        })
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> = tx.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
