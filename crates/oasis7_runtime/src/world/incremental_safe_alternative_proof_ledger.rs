//! Independent accepted-parent ProofLedger input. Not a matching permission.
use super::incremental_action_execution::SelectedPendingActionV1;
use super::incremental_body::BodyMutationError;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::{MerkleNodeReader, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_cognition_proof_ledger::{
        CognitionProofLedgerFieldV1, CurrentCognitionProofLedgerV1, ProofLedgerReadWorkV1,
        ProofLedgerRecoveredV1,
    },
    incremental_field_read::WorldFieldReadContextV1,
};
fn invalid() -> BodyMutationError {
    BodyMutationError::Invalid("SafeAlternative accepted ProofLedger")
}
pub(super) struct PaidSafeAlternativeProofLedgerV1<'a> {
    source: &'a CurrentCognitionProofLedgerV1,
    binding: Option<ProofLedgerRecoveredV1<serde_json::Value>>,
    guard: PreparedGuard,
    core: WorkIdentityV1,
    _outer: ReservedValue<()>,
}
impl PaidSafeAlternativeProofLedgerV1<'_> {
    pub(super) fn binding(&self) -> Option<&serde_json::Value> {
        self.binding.as_ref().map(|b| b.value())
    }
    pub(super) fn validate<M: MerkleNodeReader>(
        &self,
        tx: &ExecutionTransaction<'_, M>,
        common: &mut FormalBoundedContextV1<'_>,
        live: &ExecutionFence,
    ) -> Result<(), BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        tx.validate_guard(&self.guard, live)?;
        if self.core != common.work().identity() || !self._outer.uses_budget(common.memory()) {
            return Err(invalid());
        }
        self.source.validate_context(tx, common, live)?;
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn open_safe_alternative_proof_ledger_v1<'a, M, P, R, E, C>(
    source: &'a CurrentCognitionProofLedgerV1,
    selected: &SelectedPendingActionV1,
    tx: &mut ExecutionTransaction<'_, M>,
    read: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    scan: &mut StateScanBudget,
    work: &mut ProofLedgerReadWorkV1,
    common: &mut FormalBoundedContextV1<'_>,
    live: &ExecutionFence,
) -> Result<PaidSafeAlternativeProofLedgerV1<'a>, BodyMutationError>
where
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let result = (|| {
        selected.validate(tx)?;
        tx.validate_guard(&tx.guard(), live)?;
        if !tx.uses_budget(common.memory())
            || !read.values.memory.same_pool(common.memory())
            || read.values.class != MemoryClass::Ordinary
        {
            return Err(invalid());
        }
        // The Current side owner clamps all legacy lanes to this Common balance
        // and reconciles attempted work under the exclusive non-Clone child lease.
        let fee = common
            .memory()
            .reserve(
                std::mem::size_of::<PaidSafeAlternativeProofLedgerV1<'a>>(),
                MemoryClass::Ordinary,
            )
            .map_err(ValueTreeError::Memory)?;
        let binding = source.read_field(
            tx,
            CognitionProofLedgerFieldV1::RuntimeBinding,
            read,
            scan,
            work,
            common,
            live,
        )?;
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        Ok(PaidSafeAlternativeProofLedgerV1 {
            source,
            binding,
            guard: tx.guard(),
            core: common.work().identity(),
            _outer: fee.retain(()),
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
