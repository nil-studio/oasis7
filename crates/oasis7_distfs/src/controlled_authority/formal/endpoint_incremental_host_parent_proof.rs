//! Read original Qualified proof DATA from the actual accepted endpoint Store.
//! Its complete paid request state remains owned; no graph clone, detached
//! verified token or replacement parent/lease is constructed here.
use super::*;
use crate::controlled_authority::formal::endpoint::store::{
    bounded_record_id::hash_canonical_record_bounded_v1, bounded_request::PaidRequestStateV1,
};
use oasis7_formal_protocol::controlled_authority::formal::{
    FormalQualifiedCommitProofV1,
    bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    context_wire,
};
use oasis7_proto::memory_budget::MemoryReservation;

pub struct PaidHostQualifiedParentProofV1 {
    state: PaidRequestStateV1,
    record: [u8; 32],
    core: WorkIdentityV1,
    fee: MemoryReservation,
}
impl PaidHostQualifiedParentProofV1 {
    /// Original Store bytes decoded by the existing paid RequestState reader.
    /// This is DATA; the containing accepted parent/lease remains mandatory.
    pub fn proof(&self) -> &FormalQualifiedCommitProofV1 {
        match self.state.state() {
            FormalRequestStateV1::Qualified { proof, .. } => proof,
            _ => unreachable!("private Qualified-only retained proof owner"),
        }
    }
    /// Borrow the original candidate graph retained by the same paid Store
    /// request. This is location DATA; no standalone accepted permit escapes.
    pub fn material(
        &self,
    ) -> &oasis7_formal_protocol::controlled_authority::replicated_protocol::ClosedRecord {
        match self.state.state() {
            FormalRequestStateV1::Qualified { candidate, .. } => &candidate.material,
            _ => unreachable!("private Qualified-only retained material owner"),
        }
    }
    pub fn actual_record_id(&self) -> [u8; 32] {
        self.record
    }
    pub fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        common.consume_steps(1)?;
        if self.core != common.work().identity()
            || !self.state.uses_budget(common.memory())
            || !self.fee.uses_budget(common.memory())
        {
            return Err(invalid("Host parent proof foreign Core/pool"));
        }
        Ok(())
    }
    pub fn validate_parent_bounded(
        &self,
        parent: &HostAcceptedParentReadViewV1<'_>,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        self.validate_context(common)?;
        common.charge(1, 32)?;
        if self.record != parent.head().verified_record_id()
            || self.core != parent.qualified_proof_core
        {
            return Err(invalid("Host parent proof actual head changed"));
        }
        Ok(())
    }
}
impl HostAcceptedParentReadViewV1<'_> {
    pub fn read_qualified_parent_proof_bounded(
        &self,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> Result<PaidHostQualifiedParentProofV1, FormalError> {
        work.consume_steps(1, stop)?;
        let identity = work.bounded_context(memory, stop).work().identity();
        if self.qualified_proof_core != identity || !self.core.uses_budget(memory) {
            return Err(invalid("Host parent proof actual Core pool"));
        }
        let fee = memory
            .reserve(
                std::mem::size_of::<PaidHostQualifiedParentProofV1>(),
                MemoryClass::Ordinary,
            )
            .map_err(|_| invalid("Host parent proof owner admission"))?;
        let state = self
            .store
            .read_request_bounded(&self.head.record().request_digest, memory, work, stop)?
            .ok_or_else(|| invalid("Host actual parent Qualified request missing"))?;
        let FormalRequestStateV1::Qualified { proof, .. } = state.state() else {
            return Err(invalid("Host actual parent request is not Qualified"));
        };
        if !state.uses_budget(memory) {
            return Err(invalid("Host parent proof request pool"));
        }
        // This reference can only be minted by the actual Host view factory:
        // GEN/Normal retain their original paid full-quorum verdict; cold uses
        // the genuinely authenticated and restored native checkpoint proof.
        // Matching *all* proof bytes prevents forged complete/final receipts
        // from hiding behind the same canonical record ID.
        {
            let mut common = work.bounded_context(memory, stop);
            let expected = context_wire::encode_paid(
                self.qualified_proof,
                64 << 20,
                &mut common,
                "formal encoding exceeds byte bound",
                "Host accepted parent proof",
            )?;
            let actual = context_wire::encode_paid(
                proof,
                64 << 20,
                &mut common,
                "formal encoding exceeds byte bound",
                "Host stored parent proof",
            )?;
            common.charge(1, 16)?;
            if expected.len() != actual.len() {
                return Err(invalid(
                    "Host stored parent proof differs from actual verified source",
                ));
            }
            for (left, right) in expected.chunks(8192).zip(actual.chunks(8192)) {
                common.charge(
                    1,
                    (left.len() as u64)
                        .checked_mul(2)
                        .ok_or_else(|| invalid("Host proof comparison work"))?,
                )?;
                if left != right {
                    return Err(invalid(
                        "Host stored parent proof differs from actual verified source",
                    ));
                }
            }
        }
        let digest = hash_canonical_record_bounded_v1(&proof.decision.record, memory, work, stop)?;
        work.charge(1, 96, stop)?;
        let mut record = [0u8; 32];
        hex::decode_to_slice(digest.id(), &mut record)
            .map_err(|_| invalid("Host stored parent record ID encoding"))?;
        if record != self.head.verified_record_id() {
            return Err(invalid(
                "Host stored parent proof record ID differs from actual head",
            ));
        }
        let mut common = work.bounded_context(memory, stop);
        common.consume_steps(1)?;
        Ok(PaidHostQualifiedParentProofV1 {
            state,
            record,
            core: common.work().identity(),
            fee,
        })
    }
}
