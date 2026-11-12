//! The actual borrowed Runtime side producer. Disabled Cognition emits no
//! invented ledger. Its provisional capsule must still enter a real Result.
use super::World;
use oasis7_formal_protocol::{
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::{
    incremental_cognition_proof_ledger::{
        CognitionProofLedgerManifestV1, ProofLedgerWriteWorkV1,
        prepare_cognition_proof_ledger_genesis_v1,
    },
    incremental_field_member::{MemberRecordError, MemberRecordWriteContext},
};
impl World {
    pub(crate) fn prepare_iws_cognition_proof_ledger_genesis_v1<W, C>(
        &self,
        roots: &oasis7_formal_protocol::incremental_execution::RootSetV1,
        write: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut ProofLedgerWriteWorkV1,
    ) -> Result<Option<ReservedValue<CognitionProofLedgerManifestV1>>, MemberRecordError>
    where
        W: StatePageWriter + ValueNodeReader + ValueNodeWriter,
        C: FnMut() -> bool,
    {
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if self.cognition().is_null() {
            return Ok(None);
        }
        let envelope = self
            .borrow_iws_cognition_envelope_v1()
            .map_err(ValueTreeError::Invalid)?;
        let _phase = write
            .memory
            .reserve(4096, oasis7_proto::memory_budget::MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        // RootSet's original format has a 256-byte world ID and six fixed roots.
        // Invalid oversized IDs are rejected by validation without scanning them.
        // Debit its validation/copy/hash bound before the original digest call;
        // never mint a separate work balance for this metadata phase.
        let root_bytes = b"oasis7.iws.root-set.v1\0"
            .len()
            .checked_add(
                4 + 2 + 48 + 6 * oasis7_formal_protocol::incremental_state::STATE_ROOT_BYTES,
            )
            .and_then(|n| n.checked_add(roots.world_id.len().min(256)))
            .ok_or(ValueTreeError::ResourceLimited("ProofLedger GEN root hash"))?;
        let steps = u64::try_from(root_bytes)
            .ok()
            .and_then(|n| n.checked_mul(3))
            .ok_or(ValueTreeError::ResourceLimited("ProofLedger GEN root hash"))?;
        work.cells_remaining = work
            .cells_remaining
            .checked_sub(steps)
            .ok_or(ValueTreeError::ResourceLimited("ProofLedger GEN root hash"))?;
        if (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        roots
            .validate()
            .map_err(|_| ValueTreeError::Invalid("ProofLedger GEN roots"))?;

        prepare_cognition_proof_ledger_genesis_v1(
            envelope,
            roots
                .digest()
                .map_err(|_| ValueTreeError::Invalid("ProofLedger GEN roots"))?,
            write,
            work,
        )
        .map(Some)
    }
}
