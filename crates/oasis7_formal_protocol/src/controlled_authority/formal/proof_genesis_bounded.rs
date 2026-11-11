//! Fixed GEN quorum verifier. No caller-supplied verifier/hash callbacks.
#[path = "proof_genesis_build.rs"]
mod build;
use super::*;
use crate::controlled_authority::formal::bounded_context::{
    FormalBoundedContextV1, WorkIdentityV1,
};
use crate::controlled_authority::replicated_protocol::ArtifactRole;
pub use build::{build_genesis_decision_bounded, build_genesis_qualified_bounded};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{
    cell::RefCell,
    mem::{MaybeUninit, align_of, size_of},
    ptr::NonNull,
};
const LIMIT: usize = 64 * 1024 * 1024;
fn add(a: usize, b: usize) -> Result<usize, FormalError> {
    a.checked_add(b)
        .ok_or_else(|| invalid("GEN proof layout overflow"))
}
fn charge(ctx: &mut FormalBoundedContextV1<'_>, n: usize) -> Result<(), FormalError> {
    ctx.charge(1, n as u64)
}
struct Frame(ReservedValue<Vec<u8>>);
impl AsRef<[u8]> for Frame {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
fn frame<T: Serialize>(
    domain: &str,
    value: &T,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<Frame, FormalError> {
    let wire = crate::controlled_authority::formal::context_wire::encode_paid(
        value,
        LIMIT,
        ctx,
        "formal encoding exceeds byte bound",
        "GEN proof encoding",
    )?;
    let n = add(add(8, domain.len())?, wire.len())?;
    let fee = ctx
        .memory()
        .reserve(add(n, size_of::<Vec<u8>>())?, MemoryClass::Ordinary)
        .map_err(|_| invalid("GEN proof frame admission"))?;
    let mut bytes = Vec::with_capacity(n);
    if bytes.capacity() != n {
        return Err(invalid("GEN proof frame capacity"));
    }
    for part in [
        &(domain.len() as u64).to_be_bytes()[..],
        domain.as_bytes(),
        wire.as_slice(),
    ] {
        for chunk in part.chunks(8192) {
            charge(ctx, chunk.len())?;
            bytes.extend_from_slice(chunk);
        }
    }
    Ok(Frame(fee.retain(bytes)))
}
fn hash<T: Serialize>(
    domain: &str,
    value: &T,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<String>, FormalError> {
    let fee = ctx
        .memory()
        .reserve(64 + size_of::<String>(), MemoryClass::Ordinary)
        .map_err(|_| invalid("GEN proof digest admission"))?;
    let bytes = frame(domain, value, ctx)?;
    let mut hash = blake3::Hasher::new();
    for chunk in bytes.as_ref().chunks(8192) {
        charge(ctx, chunk.len())?;
        hash.update(chunk);
    }
    charge(ctx, 64)?;
    let text = hash.finalize().to_hex().to_string();
    if text.capacity() != 64 {
        return Err(invalid("GEN proof digest capacity"));
    }
    Ok(fee.retain(text))
}
fn same<T: Serialize + PartialEq>(
    a: &T,
    b: &T,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<bool, FormalError> {
    let a_wire = crate::controlled_authority::formal::context_wire::encode_paid(
        a,
        LIMIT,
        ctx,
        "formal encoding exceeds byte bound",
        "GEN proof comparison",
    )?;
    let b_wire = crate::controlled_authority::formal::context_wire::encode_paid(
        b,
        LIMIT,
        ctx,
        "formal encoding exceeds byte bound",
        "GEN proof comparison",
    )?;
    charge(ctx, add(a_wire.len(), b_wire.len())?)?;
    Ok(a == b)
}
fn verify<T: Serialize>(
    domain: &str,
    value: &T,
    signature: &str,
    key: &str,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<(), FormalError> {
    let _phase = ctx
        .memory()
        .reserve(
            4096 + 96
                + 2 * size_of::<Vec<u8>>()
                + size_of::<ed25519_dalek::VerifyingKey>()
                + size_of::<ed25519_dalek::Signature>(),
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("GEN proof crypto admission"))?;
    let shared = RefCell::new(ctx);
    crate::controlled_authority::formal::crypto::verify_with_preimage(
        signature,
        key,
        || frame(domain, value, &mut shared.borrow_mut()),
        |n| charge(&mut shared.borrow_mut(), n),
    )
}
fn copy(text: &str, ctx: &mut FormalBoundedContextV1<'_>) -> Result<String, FormalError> {
    charge(ctx, text.len())?;
    let value = text.to_owned();
    if value.capacity() != text.len() {
        return Err(invalid("GEN proof copy capacity"));
    }
    Ok(value)
}
// Locked Rust1.96 BTreeMap B=6 clone bound, same role graph as native Hot.
fn map_layout(entries: usize) -> Result<usize, FormalError> {
    if entries > 14 {
        return Err(invalid("GEN proof artifact role count"));
    }
    if entries == 0 {
        return Ok(0);
    }
    let a = align_of::<Option<NonNull<()>>>()
        .max(align_of::<u16>())
        .max(align_of::<ArtifactRole>())
        .max(align_of::<String>());
    let leaf = size_of::<Option<NonNull<()>>>()
        + size_of::<MaybeUninit<u16>>()
        + size_of::<u16>()
        + 11 * size_of::<MaybeUninit<ArtifactRole>>()
        + 11 * size_of::<MaybeUninit<String>>()
        + 5 * (a - 1);
    if entries <= 10 {
        return Ok(leaf);
    }
    Ok(3 * leaf + 12 * size_of::<NonNull<()>>() + 2 * (a - 1))
}
fn check_record_copy(record: &CanonicalRecordV1) -> Result<(), FormalError> {
    let CanonicalRecordV1 {
        schema_version: _,
        durable_set_transition_digest,
        kind: _,
        world,
        position: _,
        parent_record_id,
        original_authority_id,
        original_epoch: _,
        activation_ref,
        request_digest,
        payload_digest,
        frozen_reservation_id,
        runtime_manifest_digest,
        governing_manifest_digest,
        execution_policy_digest,
        before_state_root,
        after_state_root,
        action_root,
        event_root,
        result_root,
        projection_root,
        artifact_roots,
        handoff,
    } = record;
    let FormalWorldIdentityV1 {
        world_id,
        chain_id,
        genesis_digest,
    } = world;
    for value in [
        parent_record_id,
        original_authority_id,
        activation_ref,
        request_digest,
        payload_digest,
        runtime_manifest_digest,
        governing_manifest_digest,
        execution_policy_digest,
        before_state_root,
        after_state_root,
        action_root,
        event_root,
        result_root,
        projection_root,
        world_id,
        chain_id,
        genesis_digest,
    ]
    .into_iter()
    .chain(durable_set_transition_digest.iter())
    .chain(frozen_reservation_id.iter())
    .chain(artifact_roots.values())
    {
        if value.capacity() != value.len() {
            return Err(invalid("GEN record clone capacity"));
        }
    }
    if let Some(handoff) = handoff {
        for value in [&handoff.next_profile, &handoff.next_authority_digest] {
            if value.capacity() != value.len() {
                return Err(invalid("GEN handoff clone capacity"));
            }
        }
    }
    Ok(())
}
struct Pair {
    verified: VerifiedFormalCommitV1,
    proof: FormalQualifiedCommitProofV1,
}
pub struct PaidVerifiedGenesisCommitV1 {
    pair: ReservedValue<Pair>,
    identity: WorkIdentityV1,
}
impl PaidVerifiedGenesisCommitV1 {
    pub fn verified(&self) -> &VerifiedFormalCommitV1 {
        &self.pair.verified
    }
    pub fn proof(&self) -> &FormalQualifiedCommitProofV1 {
        &self.pair.proof
    }
    pub fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.pair.uses_budget(memory)
    }
    pub fn reserved_bytes(&self) -> usize {
        self.pair.reserved_bytes()
    }
    pub fn validate_context(
        &self,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        ctx.consume_steps(1)?;
        if !self.uses_budget(ctx.memory()) || self.identity != ctx.work().identity() {
            return Err(invalid("GEN proof foreign context"));
        }
        Ok(())
    }
}
impl std::fmt::Debug for PaidVerifiedGenesisCommitV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaidVerifiedGenesisCommitV1")
            .finish_non_exhaustive()
    }
}
pub fn verify_genesis_qualified_bounded(
    proof: &FormalQualifiedCommitProofV1,
    candidate: &SignedCanonicalCandidateV1,
    context: &VerifiedCandidateContextV1,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidVerifiedGenesisCommitV1, FormalError> {
    ctx.consume_steps(1)?;
    if proof.schema_version != 1 {
        return Err(invalid("GEN qualified proof version"));
    }
    let DecisionIds {
        id,
        finalizer,
        digest: decision,
    } = validate_genesis_decision(&proof.decision, candidate, context, ctx)?;
    let record = context.record();
    let authority = context.finalizing_authority();
    for (receipt, role) in [
        (&proof.primary_complete, EndpointRole::Primary),
        (&proof.replica_complete, EndpointRole::Replica),
    ] {
        let instance = match role {
            EndpointRole::Primary => &authority.definition().primary.instance,
            EndpointRole::Replica => &authority.definition().replica.instance,
        };
        let digest = hash_instance(instance, ctx)?;
        let _fee = ctx
            .memory()
            .reserve(
                size_of::<FormalProofCompleteBodyV1>()
                    + id.len()
                    + finalizer.len()
                    + digest.len()
                    + decision.len(),
                MemoryClass::Ordinary,
            )
            .map_err(|_| invalid("GEN complete receipt admission"))?;
        let expected = FormalProofCompleteBodyV1 {
            schema_version: 1,
            record_id: copy(&id, ctx)?,
            position: record.position,
            decision_proof_digest: copy(&decision, ctx)?,
            finalizing_authority_id: copy(&finalizer, ctx)?,
            role,
            instance_digest: copy(&digest, ctx)?,
            branch: FormalCompletionBranchV1::Normal,
            recovery_certificate_digest: None,
        };
        if !same(&receipt.body, &expected, ctx)? {
            return Err(invalid("GEN complete receipt exact context"));
        }
        verify(
            PROOF_COMPLETE_SIGN_DOMAIN_V1,
            &receipt.body,
            &receipt.signature_hex,
            &instance.endpoint_public_key,
            ctx,
        )?;
    }
    let wire = crate::controlled_authority::formal::context_wire::encode_paid(
        proof,
        LIMIT,
        ctx,
        "formal encoding exceeds byte bound",
        "GEN proof clone",
    )?;
    // CBOR embeds every String's raw UTF8 payload. Twice actual N covers the
    // proof plus its second canonical record in VerifiedCommit; map nodes are
    // independent of that payload and prepaid separately before either clone.
    let n = add(
        finalizer.len(),
        add(
            add(size_of::<PaidVerifiedGenesisCommitV1>(), size_of::<Pair>())?,
            add(
                wire.len()
                    .checked_mul(2)
                    .ok_or_else(|| invalid("GEN proof clone layout"))?,
                map_layout(record.artifact_roots.len())?
                    .checked_mul(2)
                    .ok_or_else(|| invalid("GEN proof clone layout"))?,
            )?,
        )?,
    )?;
    let fee = ctx
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(|_| invalid("GEN proof clone admission"))?;
    charge(
        ctx,
        wire.len()
            .checked_mul(2)
            .ok_or_else(|| invalid("GEN proof clone layout"))?,
    )?;
    ctx.consume_steps((2 * (record.artifact_roots.len() + 40)) as u64)?;
    let cloned_record = record.clone();
    let cloned_proof = proof.clone();
    check_record_copy(&cloned_record)?;
    check_record_copy(&cloned_proof.decision.record)?;
    charge(
        ctx,
        wire.len()
            .checked_mul(2)
            .ok_or_else(|| invalid("GEN proof comparison layout"))?,
    )?;
    if cloned_record != *record || cloned_proof != *proof {
        return Err(invalid("GEN proof clone changed value"));
    }
    charge(ctx, id.len())?;
    let mut verified_record_id = [0; 32];
    hex::decode_to_slice(id.as_str(), &mut verified_record_id)
        .map_err(|_| invalid("GEN verified record id"))?;
    let verified = VerifiedFormalCommitV1 {
        verified_record_id,
        record: cloned_record,
        finalizing_authority_id: copy(&finalizer, ctx)?,
    };
    Ok(PaidVerifiedGenesisCommitV1 {
        pair: fee.retain(Pair {
            verified,
            proof: cloned_proof,
        }),
        identity: ctx.work().identity(),
    })
}
fn hash_instance(
    value: &fencing::EndpointInstanceBindingV1,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<String>, FormalError> {
    let fee = ctx
        .memory()
        .reserve(64 + size_of::<String>(), MemoryClass::Ordinary)
        .map_err(|_| invalid("GEN instance digest admission"))?;
    let wire = crate::controlled_authority::formal::context_wire::encode_paid(
        value,
        LIMIT,
        ctx,
        "formal encoding exceeds byte bound",
        "GEN instance digest",
    )?;
    let mut hash = blake3::Hasher::new();
    for chunk in wire.chunks(8192) {
        charge(ctx, chunk.len())?;
        hash.update(chunk);
    }
    charge(ctx, 64)?;
    let text = hash.finalize().to_hex().to_string();
    if text.capacity() != 64 {
        return Err(invalid("GEN instance digest capacity"));
    }
    Ok(fee.retain(text))
}

struct DecisionIds {
    id: ReservedValue<String>,
    finalizer: ReservedValue<String>,
    digest: ReservedValue<String>,
}
fn validate_genesis_decision(
    decision: &FormalDecisionProofV1,
    candidate: &SignedCanonicalCandidateV1,
    context: &VerifiedCandidateContextV1,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<DecisionIds, FormalError> {
    ctx.consume_steps(1)?;
    let FormalDecisionProofV1 {
        schema_version: _,
        record: _,
        branch: _,
        writer_signature_hex: _,
        frozen: _,
        primary: _,
        replica: _,
    } = decision;
    let record = context.record();
    let authority = context.finalizing_authority();
    check_genesis_context(context, ctx)?;
    if decision.schema_version != 1
        || decision.branch != FormalCompletionBranchV1::Normal
        || decision.frozen.is_some()
        || !same(record, &candidate.record, ctx)?
        || !same(record, &decision.record, ctx)?
        || !same(
            &candidate.writer_signature_hex,
            &decision.writer_signature_hex,
            ctx,
        )?
    {
        return Err(invalid("GEN proof exact candidate context"));
    }
    verify(
        CANDIDATE_SIGN_DOMAIN_V1,
        record,
        &candidate.writer_signature_hex,
        &authority.definition().writer_public_key,
        ctx,
    )?;
    let id = hash(CANONICAL_RECORD_ID_DOMAIN_V1, record, ctx)?;
    let finalizer = hash(AUTHORITY_ID_DOMAIN_V1, authority.definition(), ctx)?;
    for (receipt, role) in [
        (&decision.primary, EndpointRole::Primary),
        (&decision.replica, EndpointRole::Replica),
    ] {
        let instance = match role {
            EndpointRole::Primary => &authority.definition().primary.instance,
            EndpointRole::Replica => &authority.definition().replica.instance,
        };
        let digest = hash_instance(instance, ctx)?;
        let _fee = ctx
            .memory()
            .reserve(
                size_of::<FormalFinalReceiptBodyV1>() + id.len() + finalizer.len() + digest.len(),
                MemoryClass::Ordinary,
            )
            .map_err(|_| invalid("GEN final receipt admission"))?;
        let expected = FormalFinalReceiptBodyV1 {
            schema_version: 1,
            record_id: copy(&id, ctx)?,
            position: record.position,
            finalizing_authority_id: copy(&finalizer, ctx)?,
            role,
            instance_digest: copy(&digest, ctx)?,
            branch: FormalCompletionBranchV1::Normal,
            recovery_certificate_digest: None,
        };
        if !same(&receipt.body, &expected, ctx)? {
            return Err(invalid("GEN final receipt exact context"));
        }
        verify(
            FINAL_RECEIPT_SIGN_DOMAIN_V1,
            &receipt.body,
            &receipt.signature_hex,
            &instance.endpoint_public_key,
            ctx,
        )?;
    }
    let digest = hash(DECISION_PROOF_DOMAIN_V1, decision, ctx)?;
    Ok(DecisionIds {
        id,
        finalizer,
        digest,
    })
}

fn check_genesis_context(
    context: &VerifiedCandidateContextV1,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<(), FormalError> {
    ctx.consume_steps(1)?;
    let record = context.record();
    let authority = context.finalizing_authority();
    if context.branch() != FormalCompletionBranchV1::Normal
        || context.recovery_certificate_digest().is_some()
        || record.kind != FormalRecordKindV1::InitialActivation
        || record.schema_version != 7
        || record.frozen_reservation_id.is_some()
        || authority.definition().profile_version
            != super::super::super::incremental::IWS_AUTHORITY_PROFILE_VERSION
        || authority.initial_body().is_none()
    {
        return Err(invalid(
            "GEN proof requires authenticated initial IWS candidate context",
        ));
    }
    Ok(())
}

pub fn genesis_final_receipt_body_bounded(
    candidate: &SignedCanonicalCandidateV1,
    context: &VerifiedCandidateContextV1,
    role: EndpointRole,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<FormalFinalReceiptBodyV1>, FormalError> {
    check_genesis_context(context, ctx)?;
    if !same(&candidate.record, context.record(), ctx)? {
        return Err(invalid("GEN final candidate context"));
    }
    let authority = context.finalizing_authority();
    verify(
        CANDIDATE_SIGN_DOMAIN_V1,
        context.record(),
        &candidate.writer_signature_hex,
        &authority.definition().writer_public_key,
        ctx,
    )?;
    let id = hash(CANONICAL_RECORD_ID_DOMAIN_V1, context.record(), ctx)?;
    let finalizer = hash(AUTHORITY_ID_DOMAIN_V1, authority.definition(), ctx)?;
    let instance = match role {
        EndpointRole::Primary => &authority.definition().primary.instance,
        EndpointRole::Replica => &authority.definition().replica.instance,
    };
    let digest = hash_instance(instance, ctx)?;
    let fee = ctx
        .memory()
        .reserve(
            size_of::<FormalFinalReceiptBodyV1>() + id.len() + finalizer.len() + digest.len(),
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("GEN final receipt admission"))?;
    Ok(fee.retain(FormalFinalReceiptBodyV1 {
        schema_version: 1,
        record_id: copy(&id, ctx)?,
        position: context.record().position,
        finalizing_authority_id: copy(&finalizer, ctx)?,
        role,
        instance_digest: copy(&digest, ctx)?,
        branch: FormalCompletionBranchV1::Normal,
        recovery_certificate_digest: None,
    }))
}

pub struct PaidVerifiedGenesisDecisionV1 {
    decision: ReservedValue<FormalDecisionProofV1>,
    digest: ReservedValue<String>,
    identity: WorkIdentityV1,
}
impl std::fmt::Debug for PaidVerifiedGenesisDecisionV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaidVerifiedGenesisDecisionV1")
            .finish_non_exhaustive()
    }
}
impl PaidVerifiedGenesisDecisionV1 {
    pub fn decision(&self) -> &FormalDecisionProofV1 {
        &self.decision
    }
    pub fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.decision.uses_budget(memory) && self.digest.uses_budget(memory)
    }
    pub fn reserved_bytes(&self) -> usize {
        self.decision.reserved_bytes() + self.digest.reserved_bytes()
    }
    pub fn validate_context(
        &self,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        ctx.consume_steps(1)?;
        if !self.uses_budget(ctx.memory()) || self.identity != ctx.work().identity() {
            return Err(invalid("GEN decision foreign context"));
        }
        Ok(())
    }
    pub fn complete_receipt_body_bounded(
        &self,
        role: EndpointRole,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<FormalProofCompleteBodyV1>, FormalError> {
        self.validate_context(ctx)?;
        let final_body = match role {
            EndpointRole::Primary => &self.decision.primary.body,
            EndpointRole::Replica => &self.decision.replica.body,
        };
        let fee = ctx
            .memory()
            .reserve(
                size_of::<FormalProofCompleteBodyV1>()
                    + final_body.record_id.len()
                    + self.digest.len()
                    + final_body.finalizing_authority_id.len()
                    + final_body.instance_digest.len(),
                MemoryClass::Ordinary,
            )
            .map_err(|_| invalid("GEN complete receipt admission"))?;
        Ok(fee.retain(FormalProofCompleteBodyV1 {
            schema_version: 1,
            record_id: copy(&final_body.record_id, ctx)?,
            position: final_body.position,
            decision_proof_digest: copy(&self.digest, ctx)?,
            finalizing_authority_id: copy(&final_body.finalizing_authority_id, ctx)?,
            role,
            instance_digest: copy(&final_body.instance_digest, ctx)?,
            branch: FormalCompletionBranchV1::Normal,
            recovery_certificate_digest: None,
        }))
    }
}
pub fn verify_genesis_decision_bounded(
    decision: &FormalDecisionProofV1,
    candidate: &SignedCanonicalCandidateV1,
    context: &VerifiedCandidateContextV1,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidVerifiedGenesisDecisionV1, FormalError> {
    let ids = validate_genesis_decision(decision, candidate, context, ctx)?;
    let wire = crate::controlled_authority::formal::context_wire::encode_paid(
        decision,
        LIMIT,
        ctx,
        "formal encoding exceeds byte bound",
        "GEN decision clone",
    )?;
    let n = add(
        add(
            size_of::<PaidVerifiedGenesisDecisionV1>(),
            size_of::<FormalDecisionProofV1>(),
        )?,
        add(
            wire.len(),
            map_layout(decision.record.artifact_roots.len())?,
        )?,
    )?;
    let fee = ctx
        .memory()
        .reserve(n, MemoryClass::Ordinary)
        .map_err(|_| invalid("GEN decision clone admission"))?;
    charge(ctx, wire.len())?;
    ctx.consume_steps((decision.record.artifact_roots.len() + 40) as u64)?;
    let cloned = decision.clone();
    check_record_copy(&cloned.record)?;
    charge(ctx, wire.len())?;
    if cloned != *decision {
        return Err(invalid("GEN decision clone changed value"));
    }
    Ok(PaidVerifiedGenesisDecisionV1 {
        decision: fee.retain(cloned),
        digest: ids.digest,
        identity: ctx.work().identity(),
    })
}

#[cfg(test)]
#[path = "proof_genesis_bounded_tests.rs"]
mod tests;
