//! Fixed bounded Normal quorum verifier. Every predicate remains in proof's
//! original shared kernel. This owner is not execution correctness or a lease.
use super::super::{
    bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    normal_frozen_bounded::{Frame, frame, wire},
};
use super::completion::genesis_bounded as paid;
use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::mem::size_of;
struct PaidHash(ReservedValue<String>);
impl AsRef<str> for PaidHash {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}
struct Pair {
    verified: VerifiedFormalCommitV1,
    proof: FormalQualifiedCommitProofV1,
}
pub struct PaidVerifiedNormalCommitV1 {
    pair: ReservedValue<Pair>,
    identity: WorkIdentityV1,
}
impl PaidVerifiedNormalCommitV1 {
    pub fn verified(&self) -> &VerifiedFormalCommitV1 {
        &self.pair.verified
    }
    pub fn proof(&self) -> &FormalQualifiedCommitProofV1 {
        &self.pair.proof
    }
    pub fn uses_budget(&self, m: &MemoryBudget) -> bool {
        self.pair.uses_budget(m)
    }
    pub fn reserved_bytes(&self) -> usize {
        self.pair.reserved_bytes()
    }
    pub fn validate_context(&self, c: &mut FormalBoundedContextV1<'_>) -> Result<(), FormalError> {
        c.consume_steps(1)?;
        if !self.uses_budget(c.memory()) || self.identity != c.work().identity() {
            return Err(invalid("Normal proof foreign context"));
        }
        Ok(())
    }
}
impl std::fmt::Debug for PaidVerifiedNormalCommitV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaidVerifiedNormalCommitV1")
            .finish_non_exhaustive()
    }
}
fn add(a: usize, b: usize) -> Result<usize, FormalError> {
    a.checked_add(b)
        .ok_or_else(|| invalid("Normal proof layout overflow"))
}
fn times(a: usize, b: usize) -> Result<usize, FormalError> {
    a.checked_mul(b)
        .ok_or_else(|| invalid("Normal proof layout overflow"))
}
// Rust1.96 BTree B=6: at most one allocated node per actual entry. One node
// bound includes eleven real role/String slots, parent/edge pointers and pad.
// No invented row cap: malformed maps still reach the original validator.
fn map_heap(n: usize) -> Result<usize, FormalError> {
    times(
        n,
        11 * size_of::<(
            crate::controlled_authority::replicated_protocol::ArtifactRole,
            String,
        )>() + 14 * size_of::<usize>()
            + 128,
    )
}
fn authority_wire(
    a: &VerifiedAuthorityV1,
    c: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<Vec<u8>>, FormalError> {
    wire(
        &(
            &a.definition,
            &a.policy,
            &a.activation_ref,
            &a.durable_set_activation_digest,
            &a.initial,
            &a.first_normal_position,
            &a.recovery_cut,
            &a.recovery_retained_record,
            &a.initial_input_digest,
        ),
        c,
    )
}
fn authority_extra(a: &VerifiedAuthorityV1) -> Result<usize, FormalError> {
    let mut n = times(
        a.policy.authorized_fencing_resource_bindings.len(),
        size_of::<String>(),
    )?;
    if let Some(initial) = &a.initial {
        n = add(
            n,
            times(
                initial
                    .recovery_policy
                    .authorized_fencing_resource_bindings
                    .len(),
                size_of::<String>(),
            )?,
        )?;
        n = add(n, map_heap(initial.artifact_roots.len())?)?;
    }
    add(n, size_of::<VerifiedAuthorityV1>())
}
struct Ops<'a, 'ctx> {
    ctx: &'a mut FormalBoundedContextV1<'ctx>,
}
impl Ops<'_, '_> {
    fn copy_hash(&mut self, value: ReservedValue<String>) -> Result<String, FormalError> {
        paid::copy(&value, self.ctx)
    }
    fn parent_head(&mut self, p: &VerifiedFormalCommitV1) -> Result<FormalHeadRefV1, FormalError> {
        Ok(FormalHeadRefV1 {
            position: p.record().position,
            record_id: self.record_id(p.record())?,
        })
    }
}
impl QuorumValidationOps for Ops<'_, '_> {
    fn validate_record(&mut self, r: &CanonicalRecordV1) -> Result<(), FormalError> {
        r.validate_with_work(&mut |n| self.ctx.charge(1, n as u64))
    }
    fn before_value<T: Serialize>(&mut self, v: &T) -> Result<(), FormalError> {
        let encoded = wire(v, self.ctx)?;
        self.ctx
            .charge(encoded.len() as u64 + 1, encoded.len() as u64)
    }
    fn before_context_clone(
        &mut self,
        r: &CanonicalRecordV1,
        a: &VerifiedAuthorityV1,
    ) -> Result<(), FormalError> {
        self.before_value(r)?;
        let encoded = authority_wire(a, self.ctx)?;
        self.ctx
            .charge(encoded.len() as u64 + 1, encoded.len() as u64)
    }
    fn record_id(&mut self, r: &CanonicalRecordV1) -> Result<String, FormalError> {
        self.validate_record(r)?;
        let hash = paid::hash(CANONICAL_RECORD_ID_DOMAIN_V1, r, self.ctx)?;
        self.copy_hash(hash)
    }
    fn authority_id(&mut self, a: &AuthorityDefinitionV1) -> Result<String, FormalError> {
        self.before_value(a)?;
        a.validate()?;
        let hash = paid::hash(AUTHORITY_ID_DOMAIN_V1, a, self.ctx)?;
        self.copy_hash(hash)
    }
    fn instance_id(
        &mut self,
        i: &fencing::EndpointInstanceBindingV1,
    ) -> Result<String, FormalError> {
        self.before_value(i)?;
        i.validate().map_err(|e| invalid(e.to_string()))?;
        let hash = paid::hash_instance(i, self.ctx)?;
        self.copy_hash(hash)
    }
    fn hash<T: Serialize>(&mut self, d: &str, v: &T) -> Result<String, FormalError> {
        let hash = paid::hash(d, v, self.ctx)?;
        self.copy_hash(hash)
    }
    fn verify<T: Serialize>(
        &mut self,
        d: &str,
        v: &T,
        s: &str,
        k: &str,
    ) -> Result<(), FormalError> {
        paid::verify(d, v, s, k, self.ctx)
    }
    fn parent_continuity(
        &mut self,
        a: &VerifiedAuthorityV1,
        p: &VerifiedFormalCommitV1,
        n: u64,
    ) -> Result<(), FormalError> {
        self.before_value(p.record())?;
        self.before_value(a.definition())?;
        let id = self.authority_id(a.definition())?;
        let head = self.parent_head(p)?;
        verify_normal_parent_continuity_with_ids(a, p, n, Some(&id), Some(&head))
    }
    fn frozen_certificate(
        &mut self,
        f: &FormalFrozenCertificateV1,
        a: &VerifiedAuthorityV1,
    ) -> Result<(), FormalError> {
        super::super::verify_normal_frozen_certificate_bounded(
            f,
            a.definition(),
            a.activation_ref(),
            self.ctx,
        )
        .map(|_| ())
    }
    fn frozen_matches(
        &mut self,
        f: &FormalFrozenCertificateV1,
        r: &CanonicalRecordV1,
    ) -> Result<(), FormalError> {
        self.before_value(f)?;
        self.before_value(r)?;
        f.matches_record_with(r, || {
            let measured = wire(&f.reservation.body, self.ctx)?;
            self.ctx
                .charge(measured.len() as u64, measured.len() as u64)?;
            f.reservation.body.validate()?;
            paid::hash(
                "oasis7.formal.frozen_identity.v1",
                &f.reservation.body,
                self.ctx,
            )
            .map(PaidHash)
        })
    }
    fn reservation(
        &mut self,
        f: &SignedFormalFrozenV1,
        a: &VerifiedAuthorityV1,
        p: &VerifiedFormalCommitV1,
    ) -> Result<(), FormalError> {
        let measured = wire(a.definition(), self.ctx)?;
        self.ctx
            .charge(measured.len() as u64, measured.len() as u64)?;
        a.definition().validate()?;
        let authority = self.authority_id(a.definition())?;
        let head = self.parent_head(p)?;
        let mut ops = ReservationOps {
            ctx: self.ctx,
            authority,
            head,
        };
        super::super::frozen::verify_formal_reservation_with(f, a, p, &mut ops).map(|_| ())
    }
}
struct ReservationOps<'a, 'ctx> {
    ctx: &'a mut FormalBoundedContextV1<'ctx>,
    authority: String,
    head: FormalHeadRefV1,
}
impl super::super::frozen::ReservationValidationOps for ReservationOps<'_, '_> {
    type Preimage = Frame;
    fn before_validation(
        &mut self,
        f: &SignedFormalFrozenV1,
        _: &VerifiedAuthorityV1,
        p: &VerifiedFormalCommitV1,
    ) -> Result<(), FormalError> {
        let a = wire(f, self.ctx)?;
        let b = wire(p.record(), self.ctx)?;
        self.ctx
            .charge((a.len() + b.len()) as u64, (a.len() + b.len()) as u64)
    }
    fn authority_id(&self) -> Option<&str> {
        Some(&self.authority)
    }
    fn parent_head(&self) -> Option<&FormalHeadRefV1> {
        Some(&self.head)
    }
    fn preimage(&mut self, b: &FormalFrozenBodyV1) -> Result<Frame, FormalError> {
        frame("oasis7.formal.frozen_reservation.sign.v1", b, self.ctx)
    }
    fn crypto_work(&mut self, n: usize) -> Result<(), FormalError> {
        self.ctx.charge(1, n as u64)
    }
    fn before_clone(&mut self, f: &SignedFormalFrozenV1) -> Result<(), FormalError> {
        let encoded = wire(f, self.ctx)?;
        self.ctx.charge(encoded.len() as u64, encoded.len() as u64)
    }
}
/// Only fresh Normal Execution is admitted. Recovery remains the independent
/// original CompletePrepared/ReplayFrozen API; GEN cannot enter this path.
pub fn verify_normal_qualified_bounded(
    proof: &FormalQualifiedCommitProofV1,
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidVerifiedNormalCommitV1, FormalError> {
    ctx.consume_steps(1)?;
    require_normal(
        &proof.decision.record,
        proof.decision.branch,
        proof.decision.frozen.as_ref(),
    )?;
    let encoded = wire(proof, ctx)?;
    let authority_encoded = authority_wire(authority, ctx)?;
    let record_encoded = wire(&proof.decision.record, ctx)?;
    let maps = map_heap(proof.decision.record.artifact_roots.len())?;
    let fixed = 3 * size_of::<CanonicalRecordV1>()
        + size_of::<VerifiedFormalReservationV1>()
        + 2 * size_of::<FormalFinalReceiptBodyV1>()
        + 2 * size_of::<FormalProofCompleteBodyV1>()
        + 8192;
    let phase_bytes = add(
        add(times(encoded.len(), 3)?, times(maps, 3)?)?,
        add(
            add(authority_encoded.len(), authority_extra(authority)?)?,
            fixed,
        )?,
    )?;
    let phase = ctx
        .memory()
        .reserve(phase_bytes, MemoryClass::Ordinary)
        .map_err(|_| invalid("Normal quorum phase admission"))?;
    let verified = super::completion::verify_formal_qualified_with(
        proof,
        authority,
        Some(parent),
        None,
        &mut Ops { ctx },
    )?;
    // Final pair has the complete frozen proof, both decision receipts, both
    // complete receipts, and the independently verified record. All String data
    // is present in the actual measured wire; typed headers/map nodes are extra.
    let retained = add(
        add(
            size_of::<Pair>() + size_of::<PaidVerifiedNormalCommitV1>(),
            add(encoded.len(), record_encoded.len())?,
        )?,
        add(times(maps, 2)?, verified.finalizing_authority_id.len())?,
    )?;
    let fee = ctx
        .memory()
        .reserve(retained, MemoryClass::Ordinary)
        .map_err(|_| invalid("Normal quorum retained proof admission"))?;
    ctx.charge(
        (proof.decision.record.artifact_roots.len() + 64) as u64,
        encoded.len() as u64,
    )?;
    let cloned = proof.clone();
    paid::check_record_copy(&cloned.decision.record)?;
    ctx.charge(encoded.len() as u64, encoded.len() as u64)?;
    if cloned != *proof {
        return Err(invalid("Normal quorum proof clone changed value"));
    }
    let owner = PaidVerifiedNormalCommitV1 {
        pair: fee.retain(Pair {
            verified,
            proof: cloned,
        }),
        identity: ctx.work().identity(),
    };
    drop(phase);
    ctx.consume_steps(0)?;
    Ok(owner)
}
fn require_normal(
    record: &CanonicalRecordV1,
    branch: FormalCompletionBranchV1,
    frozen: Option<&FormalFrozenCertificateV1>,
) -> Result<(), FormalError> {
    if branch != FormalCompletionBranchV1::Normal
        || record.kind != FormalRecordKindV1::Execution
        || !super::super::incremental::is_iws_record(record.schema_version)
        || frozen.is_none_or(|f| f.reservation.body.schema_version != 4)
    {
        return Err(invalid(
            "bounded Normal proof requires original schema4 IWS execution Frozen",
        ));
    }
    Ok(())
}
fn candidate_phase(
    n: usize,
    r: &CanonicalRecordV1,
    a: &VerifiedAuthorityV1,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<oasis7_proto::memory_budget::MemoryReservation, FormalError> {
    let aw = authority_wire(a, ctx)?;
    let bytes = add(
        add(times(n, 3)?, times(map_heap(r.artifact_roots.len())?, 3)?)?,
        add(
            add(aw.len(), authority_extra(a)?)?,
            3 * size_of::<CanonicalRecordV1>() + size_of::<VerifiedFormalReservationV1>() + 8192,
        )?,
    )?;
    ctx.memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(|_| invalid("Normal candidate phase admission"))
}
/// Real Normal candidate verification with the mandatory original certificate,
/// before either endpoint final receipt exists. The result is only signing DATA.
pub fn normal_final_receipt_body_bounded(
    candidate: &SignedCanonicalCandidateV1,
    frozen: &FormalFrozenCertificateV1,
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
    role: EndpointRole,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<FormalFinalReceiptBodyV1>, FormalError> {
    ctx.consume_steps(1)?;
    require_normal(
        &candidate.record,
        FormalCompletionBranchV1::Normal,
        Some(frozen),
    )?;
    let encoded = wire(&(candidate, frozen), ctx)?;
    let _phase = candidate_phase(encoded.len(), &candidate.record, authority, ctx)?;
    let context = verify_formal_candidate_with(
        candidate,
        Some(frozen),
        authority,
        Some(parent),
        None,
        FormalCompletionBranchV1::Normal,
        &mut Ops { ctx },
    )?;
    let fee = ctx
        .memory()
        .reserve(
            size_of::<FormalFinalReceiptBodyV1>() + 3 * 64,
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("Normal final receipt admission"))?;
    let body = context.receipt_body_with(role, &mut Ops { ctx })?;
    ctx.consume_steps(0)?;
    Ok(fee.retain(body))
}
/// Actual fresh Normal candidate context for the complete closure pipeline.
/// Its record and full verified-authority graph remain independently paid.
/// This is not a final/qualified commit, durable receipt or execution lease.
pub struct PaidVerifiedNormalCandidateContextV1 {
    context: ReservedValue<VerifiedCandidateContextV1>,
    identity: WorkIdentityV1,
}
impl std::fmt::Debug for PaidVerifiedNormalCandidateContextV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaidVerifiedNormalCandidateContextV1")
            .finish_non_exhaustive()
    }
}
impl PaidVerifiedNormalCandidateContextV1 {
    pub fn value(&self) -> &VerifiedCandidateContextV1 {
        &self.context
    }
    pub fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.context.uses_budget(memory)
    }
    pub fn reserved_bytes(&self) -> usize {
        self.context.reserved_bytes()
    }
    pub fn validate_context(
        &self,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        ctx.consume_steps(1)?;
        if !self.uses_budget(ctx.memory()) || self.identity != ctx.work().identity() {
            return Err(invalid("Normal candidate foreign context"));
        }
        Ok(())
    }
}
pub fn normal_candidate_context_bounded(
    candidate: &SignedCanonicalCandidateV1,
    frozen: &FormalFrozenCertificateV1,
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidVerifiedNormalCandidateContextV1, FormalError> {
    ctx.consume_steps(1)?;
    require_normal(
        &candidate.record,
        FormalCompletionBranchV1::Normal,
        Some(frozen),
    )?;
    let encoded = wire(&(candidate, frozen), ctx)?;
    let aw = authority_wire(authority, ctx)?;
    let rw = wire(&candidate.record, ctx)?;
    let phase = candidate_phase(encoded.len(), &candidate.record, authority, ctx)?;
    let context = verify_formal_candidate_with(
        candidate,
        Some(frozen),
        authority,
        Some(parent),
        None,
        FormalCompletionBranchV1::Normal,
        &mut Ops { ctx },
    )?;
    // The private kernel already cloned the genuine record and finalizer under
    // phase admission. Transfer those same graphs, never a bare cloned DTO.
    // String payload is in the actual wire; BTree/Vec nodes are extra typed
    // layout. Authority includes its real policy, initial body and lineage.
    let retained = add(
        size_of::<PaidVerifiedNormalCandidateContextV1>() + size_of::<VerifiedCandidateContextV1>(),
        add(
            add(rw.len(), map_heap(candidate.record.artifact_roots.len())?)?,
            add(aw.len(), authority_extra(authority)?)?,
        )?,
    )?;
    let fee = ctx
        .memory()
        .reserve(retained, MemoryClass::Ordinary)
        .map_err(|_| invalid("Normal candidate retained context admission"))?;
    ctx.consume_steps(0)?;
    let owner = PaidVerifiedNormalCandidateContextV1 {
        context: fee.retain(context),
        identity: ctx.work().identity(),
    };
    drop(phase);
    Ok(owner)
}

struct DecisionPair {
    decision: FormalDecisionProofV1,
    verified: VerifiedFormalDecisionV1,
}
pub struct PaidVerifiedNormalDecisionV1 {
    pair: ReservedValue<DecisionPair>,
    identity: WorkIdentityV1,
}
impl std::fmt::Debug for PaidVerifiedNormalDecisionV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaidVerifiedNormalDecisionV1")
            .finish_non_exhaustive()
    }
}
impl PaidVerifiedNormalDecisionV1 {
    pub fn decision(&self) -> &FormalDecisionProofV1 {
        &self.pair.decision
    }
    pub fn uses_budget(&self, m: &MemoryBudget) -> bool {
        self.pair.uses_budget(m)
    }
    pub fn reserved_bytes(&self) -> usize {
        self.pair.reserved_bytes()
    }
    pub fn validate_context(&self, c: &mut FormalBoundedContextV1<'_>) -> Result<(), FormalError> {
        c.consume_steps(1)?;
        if !self.uses_budget(c.memory()) || self.identity != c.work().identity() {
            return Err(invalid("Normal decision foreign context"));
        }
        Ok(())
    }
    pub fn complete_receipt_body_bounded(
        &self,
        role: EndpointRole,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<FormalProofCompleteBodyV1>, FormalError> {
        self.validate_context(ctx)?;
        let _phase = ctx
            .memory()
            .reserve(8192, MemoryClass::Ordinary)
            .map_err(|_| invalid("Normal completion phase admission"))?;
        let fee = ctx
            .memory()
            .reserve(
                size_of::<FormalProofCompleteBodyV1>() + 4 * 64,
                MemoryClass::Ordinary,
            )
            .map_err(|_| invalid("Normal complete receipt admission"))?;
        let body = self
            .pair
            .verified
            .complete_receipt_body_with(role, &mut Ops { ctx })?;
        ctx.consume_steps(0)?;
        Ok(fee.retain(body))
    }
}
pub fn verify_normal_decision_bounded(
    decision: &FormalDecisionProofV1,
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidVerifiedNormalDecisionV1, FormalError> {
    ctx.consume_steps(1)?;
    require_normal(&decision.record, decision.branch, decision.frozen.as_ref())?;
    let encoded = wire(decision, ctx)?;
    let aw = authority_wire(authority, ctx)?;
    let rw = wire(&decision.record, ctx)?;
    let _phase = candidate_phase(encoded.len(), &decision.record, authority, ctx)?;
    let verified =
        verify_formal_decision_with(decision, authority, Some(parent), None, &mut Ops { ctx })?;
    let n = add(
        add(
            size_of::<DecisionPair>() + size_of::<PaidVerifiedNormalDecisionV1>(),
            add(encoded.len(), rw.len())?,
        )?,
        add(
            add(aw.len(), authority_extra(authority)?)?,
            add(
                times(map_heap(decision.record.artifact_roots.len())?, 2)?,
                64,
            )?,
        )?,
    )?;
    let fee = ctx
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(|_| invalid("Normal decision clone admission"))?;
    ctx.charge(
        (decision.record.artifact_roots.len() + 64) as u64,
        encoded.len() as u64,
    )?;
    let cloned = decision.clone();
    paid::check_record_copy(&cloned.record)?;
    ctx.charge(encoded.len() as u64, encoded.len() as u64)?;
    if cloned != *decision {
        return Err(invalid("Normal decision clone changed value"));
    }
    ctx.consume_steps(0)?;
    Ok(PaidVerifiedNormalDecisionV1 {
        pair: fee.retain(DecisionPair {
            decision: cloned,
            verified,
        }),
        identity: ctx.work().identity(),
    })
}
#[cfg(test)]
#[path = "proof_normal_bounded_tests.rs"]
mod tests;
