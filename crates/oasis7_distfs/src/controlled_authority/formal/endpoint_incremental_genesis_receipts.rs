//! Real GEN-only prepare/final/complete producers, never a caller-selected signing API.
use super::super::FormalArchiveReadWorkV1;
use super::*;
use crate::controlled_authority::fencing::{
    self, BoundedAcquireErrorV1, ProtectedEndpointSignaturePurposeV1,
    RemoteMonotonicEndpointProvider, TrustedFencingPolicyV1,
};
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::{
    FormalBoundedContextV1, WorkIdentityV1,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::mem::size_of;
type ResultV<T> = Result<T, BoundedAcquireErrorV1>;
fn add(a: usize, b: usize) -> Result<usize, FormalError> {
    a.checked_add(b)
        .ok_or_else(|| invalid("GEN receipt layout"))
}
fn fee(
    memory: &MemoryBudget,
    n: usize,
) -> Result<oasis7_proto::memory_budget::MemoryReservation, FormalError> {
    memory
        .reserve(n, MemoryClass::Ordinary)
        .map_err(|_| invalid("GEN receipt admission"))
}
fn same<T: serde::Serialize + PartialEq>(
    a: &T,
    b: &T,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<bool, FormalError> {
    store::bounded_transaction::same(a, b, memory, work, stop)
}
enum ReceiptBody {
    Prepare(ReservedValue<FormalPrepareReceiptBodyV1>),
    Final(ReservedValue<FormalFinalReceiptBodyV1>),
    Complete(ReservedValue<FormalProofCompleteBodyV1>),
}
impl ReceiptBody {
    fn purpose(&self) -> ProtectedEndpointSignaturePurposeV1 {
        match self {
            Self::Prepare(_) => ProtectedEndpointSignaturePurposeV1::PrepareReceipt,
            Self::Final(_) => ProtectedEndpointSignaturePurposeV1::FinalReceipt,
            Self::Complete(_) => ProtectedEndpointSignaturePurposeV1::ProofComplete,
        }
    }
    fn wire(
        &self,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<Vec<u8>>, FormalError> {
        match self {
            Self::Prepare(v) => store::context_wire::encode_paid(
                &**v,
                64 * 1024,
                ctx,
                "GEN receipt wire bound",
                "GEN prepare body",
            ),
            Self::Final(v) => store::context_wire::encode_paid(
                &**v,
                64 * 1024,
                ctx,
                "GEN receipt wire bound",
                "GEN final body",
            ),
            Self::Complete(v) => store::context_wire::encode_paid(
                &**v,
                64 * 1024,
                ctx,
                "GEN receipt wire bound",
                "GEN complete body",
            ),
        }
    }
    fn uses_budget(&self, m: &MemoryBudget) -> bool {
        match self {
            Self::Prepare(v) => v.uses_budget(m),
            Self::Final(v) => v.uses_budget(m),
            Self::Complete(v) => v.uses_budget(m),
        }
    }
    fn typed_size(&self) -> usize {
        match self {
            Self::Prepare(_) => size_of::<FormalPrepareReceiptBodyV1>(),
            Self::Final(_) => size_of::<FormalFinalReceiptBodyV1>(),
            Self::Complete(_) => size_of::<FormalProofCompleteBodyV1>(),
        }
    }
}
pub(in crate::controlled_authority) struct PaidGenesisReceiptSourceV1<'a> {
    body: ReceiptBody,
    instance: &'a fencing::EndpointInstanceBindingV1,
    instance_digest: ReservedValue<String>,
    before: fencing::EndpointCheckpointBindingV1,
    before_fee: ReservedValue<()>,
    preimage: ReservedValue<Vec<u8>>,
    identity: WorkIdentityV1,
    outer: ReservedValue<()>,
}
impl PaidGenesisReceiptSourceV1<'_> {
    pub(in crate::controlled_authority) fn purpose(&self) -> ProtectedEndpointSignaturePurposeV1 {
        self.body.purpose()
    }
    pub(in crate::controlled_authority) fn instance(&self) -> &fencing::EndpointInstanceBindingV1 {
        self.instance
    }
    pub(in crate::controlled_authority) fn instance_digest(&self) -> &str {
        &self.instance_digest
    }
    pub(in crate::controlled_authority) fn checkpoint(
        &self,
    ) -> &fencing::EndpointCheckpointBindingV1 {
        &self.before
    }
    pub(in crate::controlled_authority) fn preimage(&self) -> &[u8] {
        &self.preimage
    }
    pub(in crate::controlled_authority) fn validate_context(
        &self,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        ctx.consume_steps(1)?;
        let m = ctx.memory();
        if self.identity != ctx.work().identity()
            || !self.body.uses_budget(m)
            || !self.instance_digest.uses_budget(m)
            || !self.before_fee.uses_budget(m)
            || !self.preimage.uses_budget(m)
            || !self.outer.uses_budget(m)
        {
            return Err(invalid("GEN receipt source foreign context"));
        }
        Ok(())
    }
}
fn source<'a>(
    body: ReceiptBody,
    instance: &'a fencing::EndpointInstanceBindingV1,
    store: &store::Store,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PaidGenesisReceiptSourceV1<'a>, FormalError> {
    let instance_digest = store::bounded_reopen::hash_encoded(instance, None, memory, work, stop)?;
    let (before, before_fee) =
        store::bounded_transaction::copy_checkpoint(&store.checkpoint, memory, work, stop)?;
    let mut ctx = work.bounded_context(memory, stop);
    let wire = body.wire(&mut ctx)?;
    let domain = body.purpose().domain();
    let n = add(add(8, domain.len())?, wire.len())?;
    if n > 64 * 1024 {
        return Err(invalid("GEN receipt preimage bound"));
    }
    let reservation = fee(memory, add(size_of::<Vec<u8>>(), n)?)?;
    let mut bytes = Vec::with_capacity(n);
    if bytes.capacity() > n {
        return Err(invalid("GEN receipt frame capacity"));
    }
    for segment in [
        &(domain.len() as u64).to_be_bytes()[..],
        domain.as_bytes(),
        wire.as_slice(),
    ] {
        for chunk in segment.chunks(8192) {
            ctx.charge(1, chunk.len() as u64)?;
            bytes.extend_from_slice(chunk);
        }
    }
    let covered = add(
        add(
            body.typed_size(),
            size_of::<fencing::EndpointCheckpointBindingV1>(),
        )?,
        add(size_of::<Vec<u8>>(), size_of::<String>())?,
    )?;
    let outer = fee(
        memory,
        size_of::<PaidGenesisReceiptSourceV1<'a>>()
            .checked_sub(covered)
            .ok_or_else(|| invalid("GEN receipt source shell"))?,
    )?
    .retain(());
    Ok(PaidGenesisReceiptSourceV1 {
        body,
        instance,
        instance_digest,
        before,
        before_fee,
        preimage: reservation.retain(bytes),
        identity: ctx.work().identity(),
        outer,
    })
}
#[derive(Debug)]
pub struct PaidGenesisPrepareReceiptV1 {
    value: ReservedValue<FormalPrepareReceiptV1>,
}
#[derive(Debug)]
pub struct PaidGenesisFinalReceiptV1 {
    value: ReservedValue<FormalFinalReceiptV1>,
}
#[derive(Debug)]
pub struct PaidGenesisCompleteReceiptV1 {
    value: ReservedValue<FormalProofCompleteReceiptV1>,
}
impl PaidGenesisPrepareReceiptV1 {
    pub fn receipt(&self) -> &FormalPrepareReceiptV1 {
        &self.value
    }
}
impl PaidGenesisFinalReceiptV1 {
    pub fn receipt(&self) -> &FormalFinalReceiptV1 {
        &self.value
    }
}
impl PaidGenesisCompleteReceiptV1 {
    pub fn receipt(&self) -> &FormalProofCompleteReceiptV1 {
        &self.value
    }
}
fn prepare_body(
    candidate: &FormalCandidateMaterialV1,
    context: &VerifiedCandidateContextV1,
    role: EndpointRole,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<ReservedValue<FormalPrepareReceiptBodyV1>, FormalError> {
    let mut ctx = work.bounded_context(memory, stop);
    let final_body =
        oasis7_formal_protocol::controlled_authority::formal::genesis_final_receipt_body_bounded(
            &candidate.candidate,
            context,
            role,
            &mut ctx,
        )?;
    drop(ctx);
    let material = store::bounded_reopen::hash_encoded(
        candidate,
        Some(super::super::types::CANDIDATE_MATERIAL_DOMAIN_V1),
        memory,
        work,
        stop,
    )?;
    let n = add(size_of::<FormalPrepareReceiptBodyV1>(), 4 * 64)?;
    let reserved = fee(memory, n)?;
    work.charge(1, 4 * 64, stop)?;
    Ok(reserved.retain(FormalPrepareReceiptBodyV1 {
        final_receipt: (*final_body).clone(),
        candidate_material_digest: (*material).clone(),
    }))
}
fn verify_prepare_one(
    receipt: &FormalPrepareReceiptV1,
    role: EndpointRole,
    candidate: &FormalCandidateMaterialV1,
    context: &VerifiedCandidateContextV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<(), FormalError> {
    let expected = prepare_body(candidate, context, role, memory, work, stop)?;
    if !same(&receipt.body, &*expected, memory, work, stop)? {
        return Err(invalid("formal prepare receipt candidate/context"));
    }
    let key = match role {
        EndpointRole::Primary => {
            &context
                .finalizing_authority()
                .definition()
                .primary
                .instance
                .endpoint_public_key
        }
        EndpointRole::Replica => {
            &context
                .finalizing_authority()
                .definition()
                .replica
                .instance
                .endpoint_public_key
        }
    };
    let mut ctx = work.bounded_context(memory, stop);
    let shared = std::cell::RefCell::new(&mut ctx);
    let diagnostics = [
        "formal digest/key requires canonical lowercase hex32",
        "formal signature requires canonical lowercase hex64",
        "formal public key",
        "formal signature",
        "formal signature verification",
    ]
    .iter()
    .try_fold(
        size_of::<FormalError>() + 2 * size_of::<String>(),
        |n, s| add(n, s.len()),
    )?;
    let _crypto = fee(
        memory,
        add(
            96 + 2 * size_of::<Vec<u8>>()
                + 96
                + size_of::<ed25519_dalek::VerifyingKey>()
                + size_of::<ed25519_dalek::Signature>(),
            diagnostics,
        )?,
    )?;
    oasis7_formal_protocol::controlled_authority::formal::crypto::verify_with_preimage(
        &receipt.signature_hex,
        key,
        || {
            let wire = store::context_wire::encode_paid(
                &receipt.body,
                64 * 1024,
                &mut shared.borrow_mut(),
                "GEN receipt body bound",
                "GEN prepare verify",
            )?;
            frame(
                ProtectedEndpointSignaturePurposeV1::PrepareReceipt.domain(),
                &wire,
                &mut shared.borrow_mut(),
            )
        },
        |n| shared.borrow_mut().charge(1, n as u64),
    )?;
    Ok(())
}
fn verify_prepare_pair(
    primary: &FormalPrepareReceiptV1,
    replica: &FormalPrepareReceiptV1,
    candidate: &FormalCandidateMaterialV1,
    context: &VerifiedCandidateContextV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<(), FormalError> {
    verify_prepare_one(
        primary,
        EndpointRole::Primary,
        candidate,
        context,
        memory,
        work,
        stop,
    )?;
    verify_prepare_one(
        replica,
        EndpointRole::Replica,
        candidate,
        context,
        memory,
        work,
        stop,
    )
}

struct Frame(ReservedValue<Vec<u8>>);
impl AsRef<[u8]> for Frame {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
fn frame(
    domain: &str,
    wire: &[u8],
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<Frame, FormalError> {
    let n = add(add(8, domain.len())?, wire.len())?;
    let reserved = fee(ctx.memory(), add(size_of::<Vec<u8>>(), n)?)?;
    let mut bytes = Vec::with_capacity(n);
    if bytes.capacity() > n {
        return Err(invalid("GEN receipt frame capacity"));
    }
    for segment in [
        &(domain.len() as u64).to_be_bytes()[..],
        domain.as_bytes(),
        wire,
    ] {
        for chunk in segment.chunks(8192) {
            ctx.charge(1, chunk.len() as u64)?;
            bytes.extend_from_slice(chunk);
        }
    }
    Ok(Frame(reserved.retain(bytes)))
}

fn admit(
    endpoint: &mut FileFormalEndpoint,
    candidate: &FormalCandidateMaterialV1,
    context: &VerifiedCandidateContextV1,
    provider: &RemoteMonotonicEndpointProvider,
    policy: &TrustedFencingPolicyV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> ResultV<()> {
    work.consume_steps(1, stop)?;
    if endpoint.authority.definition().profile_version!=oasis7_formal_protocol::controlled_authority::formal::incremental::IWS_AUTHORITY_PROFILE_VERSION || endpoint.parent.is_some() || endpoint.recovery.is_some() || candidate.candidate.record.schema_version!=7 || candidate.candidate.record.kind!=FormalRecordKindV1::InitialActivation || candidate.branch!=FormalCompletionBranchV1::Normal || candidate.frozen.is_some() || candidate.frozen_material.is_some() || context.branch()!=FormalCompletionBranchV1::Normal{return Err(invalid("GEN receipt exact initial source required").into());}
    if !same(
        &candidate.candidate.record,
        context.record(),
        memory,
        work,
        stop,
    )? || !same(
        context.finalizing_authority().definition(),
        endpoint.authority.definition(),
        memory,
        work,
        stop,
    )? || !same(
        &context.finalizing_authority().activation_ref(),
        &endpoint.authority.activation_ref(),
        memory,
        work,
        stop,
    )? || !same(
        &policy.provider_public_key,
        &endpoint.authority.policy().fencing_provider_public_key,
        memory,
        work,
        stop,
    )? || !same(
        &policy.authorized_resource_bindings,
        &endpoint
            .authority
            .policy()
            .authorized_fencing_resource_bindings,
        memory,
        work,
        stop,
    )? {
        return Err(FormalError::FencingUnproven.into());
    }
    let capability = endpoint
        .iws_page_store
        .as_ref()
        .ok_or_else(|| invalid("GEN receipt real page capability required"))?;
    if !capability.jobs.matches(candidate, &mut || stop())? {
        return Err(invalid("GEN receipt complete exact material required").into());
    }
    let id = store::bounded_record_id::hash_canonical_record_bounded_v1(
        &candidate.candidate.record,
        memory,
        work,
        stop,
    )?;
    let mut bytes = [0; 32];
    work.charge(1, id.id().len() as u64, stop)?;
    hex::decode_to_slice(id.id(), &mut bytes)
        .map_err(|_| invalid("GEN receipt candidate record ID"))?;
    capability
        .jobs
        .genesis_job()?
        .receipt_binding(bytes, memory)?;
    let instance = match endpoint.role {
        EndpointRole::Primary => &endpoint.authority.definition().primary.instance,
        EndpointRole::Replica => &endpoint.authority.definition().replica.instance,
    };
    endpoint.store.reconcile_pending_update_with_scope(
        provider,
        instance,
        policy,
        memory,
        work,
        stop,
        &store::bounded_transaction::RecoveryScope::Cold {
            authority: &endpoint.authority,
            role: endpoint.role,
        },
    )?;
    work.charge(
        1,
        (endpoint.store.body.head.record_id.len()
            + candidate.candidate.record.parent_record_id.len()) as u64,
        stop,
    )?;
    if endpoint.store.body.head.position != 0
        || endpoint.store.body.head.record_id != candidate.candidate.record.parent_record_id
        || endpoint.store.body.head_node.is_some()
    {
        return Err(invalid("GEN receipt exact initial native parent").into());
    }
    if let Some(pending) = &endpoint.store.body.pending {
        let raw = endpoint
            .store
            .raw_object_bounded(pending, 64 * 1024 * 1024, memory, work, stop)?
            .ok_or_else(|| invalid("GEN retained pending missing"))?;
        let state = store::bounded_request::decode_request_bounded_v1(&raw, memory, work, stop)?;
        if !same(
            &state.state().request_digest(),
            &candidate.candidate.record.request_digest.as_str(),
            memory,
            work,
            stop,
        )? {
            return Err(FormalError::Conflict("formal one durable reservation slot".into()).into());
        }
    }
    Ok(())
}
enum Next<'a> {
    Prepared {
        candidate: &'a FormalCandidateMaterialV1,
        receipt: Option<&'a FormalPrepareReceiptV1>,
    },
    Decided {
        candidate: &'a FormalCandidateMaterialV1,
        prepare: &'a FormalPrepareReceiptV1,
        receipt: &'a FormalFinalReceiptV1,
    },
    Ready {
        candidate: &'a FormalCandidateMaterialV1,
        prepare: &'a FormalPrepareReceiptV1,
        decision: &'a FormalDecisionProofV1,
        complete: Option<&'a FormalProofCompleteReceiptV1>,
    },
}
fn component<T: serde::Serialize>(
    value: &T,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<usize, FormalError> {
    Ok(store::context_wire::encode_paid(
        value,
        64 * 1024 * 1024,
        ctx,
        "formal object byte limit",
        "GEN state component",
    )?
    .len())
}
fn next_state(
    next: Next<'_>,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<store::PaidRequestStateV1, FormalError> {
    let mut ctx = work.bounded_context(memory, stop);
    // Original named enum/struct envelopes, all fixed labels shorter than24.
    let n = match &next {
        Next::Prepared { candidate, receipt } => add(
            1 + (1 + "Prepared".len()) + 1 + (1 + "candidate".len()) + (1 + "receipt".len()),
            add(
                component(candidate, &mut ctx)?,
                component(receipt, &mut ctx)?,
            )?,
        )?,
        Next::Decided {
            candidate,
            prepare,
            receipt,
        } => add(
            1 + (1 + "Decided".len())
                + 1
                + (1 + "candidate".len())
                + (1 + "prepare".len())
                + (1 + "receipt".len()),
            add(
                component(candidate, &mut ctx)?,
                add(component(prepare, &mut ctx)?, component(receipt, &mut ctx)?)?,
            )?,
        )?,
        Next::Ready {
            candidate,
            prepare,
            decision,
            complete,
        } => add(
            1 + (1 + "Ready".len())
                + 1
                + (1 + "candidate".len())
                + (1 + "prepare".len())
                + (1 + "decision".len())
                + (1 + "complete".len()),
            add(
                component(candidate, &mut ctx)?,
                add(
                    component(prepare, &mut ctx)?,
                    add(
                        component(decision, &mut ctx)?,
                        component(complete, &mut ctx)?,
                    )?,
                )?,
            )?,
        )?,
    };
    let reserve = fee(memory, store::bounded_request::decode_workspace(n)?)?;
    ctx.charge(1, n as u64)?;
    let value = match next {
        Next::Prepared { candidate, receipt } => FormalRequestStateV1::Prepared {
            candidate: candidate.clone(),
            receipt: receipt.cloned(),
        },
        Next::Decided {
            candidate,
            prepare,
            receipt,
        } => FormalRequestStateV1::Decided {
            candidate: candidate.clone(),
            prepare: prepare.clone(),
            receipt: receipt.clone(),
        },
        Next::Ready {
            candidate,
            prepare,
            decision,
            complete,
        } => FormalRequestStateV1::Ready {
            candidate: candidate.clone(),
            prepare: prepare.clone(),
            decision: decision.clone(),
            complete: complete.cloned(),
        },
    };
    let value = reserve.retain(value);
    let wire = store::context_wire::encode_paid(
        &*value,
        64 * 1024 * 1024,
        &mut ctx,
        "formal object byte limit",
        "GEN next state",
    )?;
    drop(ctx);
    store::bounded_request::decode_request_bytes_bounded_v1(&wire, memory, work, stop)
}
fn persist(
    store: &mut store::Store,
    next: Next<'_>,
    provider: &RemoteMonotonicEndpointProvider,
    instance: &fencing::EndpointInstanceBindingV1,
    policy: &TrustedFencingPolicyV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> ResultV<()> {
    let state = next_state(next, memory, work, stop)?;
    let update = store.prepare_request_update_bounded(&state, memory, work, stop)?;
    store.publish_pending_update_bounded(update, provider, instance, policy, memory, work, stop)
}
fn sign<'a>(
    provider: &RemoteMonotonicEndpointProvider,
    source: PaidGenesisReceiptSourceV1<'a>,
    policy: &TrustedFencingPolicyV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> ResultV<fencing::PaidGenesisReceiptSignatureV1<'a>> {
    let mut ctx = work.bounded_context(memory, stop);
    let lease = fencing::acquire_provider_lease_bounded_v1(
        provider,
        source.instance(),
        source.checkpoint(),
        policy,
        &mut ctx,
    )?;
    fencing::sign_genesis_receipt_bounded_v1(provider, lease, source, &mut ctx)
}
fn verify_body_signature<T: serde::Serialize>(
    body: &T,
    signature: &str,
    key: &str,
    purpose: ProtectedEndpointSignaturePurposeV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<(), FormalError> {
    let diagnostics = [
        "formal digest/key requires canonical lowercase hex32",
        "formal signature requires canonical lowercase hex64",
        "formal public key",
        "formal signature",
        "formal signature verification",
    ]
    .iter()
    .try_fold(
        size_of::<FormalError>() + 2 * size_of::<String>(),
        |n, s| add(n, s.len()),
    )?;
    let _crypto = fee(
        memory,
        add(
            96 + 2 * size_of::<Vec<u8>>()
                + 96
                + size_of::<ed25519_dalek::VerifyingKey>()
                + size_of::<ed25519_dalek::Signature>(),
            diagnostics,
        )?,
    )?;
    let mut ctx = work.bounded_context(memory, stop);
    let shared = std::cell::RefCell::new(&mut ctx);
    oasis7_formal_protocol::controlled_authority::formal::crypto::verify_with_preimage(
        signature,
        key,
        || {
            let wire = store::context_wire::encode_paid(
                body,
                64 * 1024,
                &mut shared.borrow_mut(),
                "GEN receipt body bound",
                "GEN retained receipt verify",
            )?;
            frame(purpose.domain(), &wire, &mut shared.borrow_mut())
        },
        |n| shared.borrow_mut().charge(1, n as u64),
    )
}
fn verify_final(
    receipt: &FormalFinalReceiptV1,
    candidate: &FormalCandidateMaterialV1,
    context: &VerifiedCandidateContextV1,
    role: EndpointRole,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<(), FormalError> {
    let mut ctx = work.bounded_context(memory, stop);
    let expected =
        oasis7_formal_protocol::controlled_authority::formal::genesis_final_receipt_body_bounded(
            &candidate.candidate,
            context,
            role,
            &mut ctx,
        )?;
    drop(ctx);
    if !same(&receipt.body, &*expected, memory, work, stop)? {
        return Err(invalid("GEN retained final context mismatch"));
    }
    let key = match role {
        EndpointRole::Primary => {
            &context
                .finalizing_authority()
                .definition()
                .primary
                .instance
                .endpoint_public_key
        }
        EndpointRole::Replica => {
            &context
                .finalizing_authority()
                .definition()
                .replica
                .instance
                .endpoint_public_key
        }
    };
    verify_body_signature(
        &receipt.body,
        &receipt.signature_hex,
        key,
        ProtectedEndpointSignaturePurposeV1::FinalReceipt,
        memory,
        work,
        stop,
    )
}
fn copy_prepare(
    body: &FormalPrepareReceiptBodyV1,
    sig: &str,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PaidGenesisPrepareReceiptV1, FormalError> {
    let mut ctx = work.bounded_context(memory, stop);
    let n = component(body, &mut ctx)?;
    let reserved = fee(
        memory,
        add(add(size_of::<PaidGenesisPrepareReceiptV1>(), n)?, sig.len())?,
    )?;
    ctx.charge(1, add(n, sig.len())? as u64)?;
    Ok(PaidGenesisPrepareReceiptV1 {
        value: reserved.retain(FormalPrepareReceiptV1 {
            body: body.clone(),
            signature_hex: sig.to_owned(),
        }),
    })
}
fn copy_final(
    body: &FormalFinalReceiptBodyV1,
    sig: &str,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PaidGenesisFinalReceiptV1, FormalError> {
    let mut ctx = work.bounded_context(memory, stop);
    let n = component(body, &mut ctx)?;
    let reserved = fee(
        memory,
        add(add(size_of::<PaidGenesisFinalReceiptV1>(), n)?, sig.len())?,
    )?;
    ctx.charge(1, add(n, sig.len())? as u64)?;
    Ok(PaidGenesisFinalReceiptV1 {
        value: reserved.retain(FormalFinalReceiptV1 {
            body: body.clone(),
            signature_hex: sig.to_owned(),
        }),
    })
}
fn copy_complete(
    body: &FormalProofCompleteBodyV1,
    sig: &str,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PaidGenesisCompleteReceiptV1, FormalError> {
    let mut ctx = work.bounded_context(memory, stop);
    let n = component(body, &mut ctx)?;
    let reserved = fee(
        memory,
        add(
            add(size_of::<PaidGenesisCompleteReceiptV1>(), n)?,
            sig.len(),
        )?,
    )?;
    ctx.charge(1, add(n, sig.len())? as u64)?;
    Ok(PaidGenesisCompleteReceiptV1 {
        value: reserved.retain(FormalProofCompleteReceiptV1 {
            body: body.clone(),
            signature_hex: sig.to_owned(),
        }),
    })
}
fn retry_state(
    endpoint: &FileFormalEndpoint,
    candidate: &FormalCandidateMaterialV1,
    context: &VerifiedCandidateContextV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<Option<store::PaidRequestStateV1>, FormalError> {
    work.consume_steps(1, stop)?;
    if endpoint.authority.definition().profile_version!=oasis7_formal_protocol::controlled_authority::formal::incremental::IWS_AUTHORITY_PROFILE_VERSION || candidate.candidate.record.schema_version!=7 || candidate.candidate.record.kind!=FormalRecordKindV1::InitialActivation || candidate.frozen.is_some() || candidate.frozen_material.is_some() || candidate.branch!=FormalCompletionBranchV1::Normal{return Err(invalid("GEN retry exact initial source required"));}
    if !same(
        &candidate.candidate.record,
        context.record(),
        memory,
        work,
        stop,
    )? || !same(
        context.finalizing_authority().definition(),
        endpoint.authority.definition(),
        memory,
        work,
        stop,
    )? || !same(
        &context.finalizing_authority().activation_ref(),
        &endpoint.authority.activation_ref(),
        memory,
        work,
        stop,
    )? {
        return Err(FormalError::FencingUnproven);
    }
    let state = endpoint.store.read_request_bounded(
        &candidate.candidate.record.request_digest,
        memory,
        work,
        stop,
    )?;
    if let Some(state) = &state {
        let old = match state.state() {
            FormalRequestStateV1::Prepared { candidate, .. }
            | FormalRequestStateV1::Decided { candidate, .. }
            | FormalRequestStateV1::Ready { candidate, .. }
            | FormalRequestStateV1::Qualified { candidate, .. } => candidate,
            _ => return Err(invalid("GEN retained request state")),
        };
        if !same(old, candidate, memory, work, stop)? {
            return Err(FormalError::Conflict(
                "GEN retained original candidate".into(),
            ));
        }
    }
    Ok(state)
}

#[path = "endpoint_incremental_genesis_receipts_execute.rs"]
mod execute;
