//! Protected NativeCheckpoint signing on the original private paid TCP lease.
//! A verified signature is data, not native publication or execution authority.
use super::*;
use crate::controlled_authority::formal::PaidNativeCheckpointDraftV1;
use std::mem::size_of;

fn challenge_paid(ctx: &mut FormalBoundedContextV1<'_>) -> ResultV<String> {
    super::super::super::fresh_challenge_with_reader(|bytes| {
        step(ctx)?;
        let mut source =
            std::fs::File::open("/dev/urandom").map_err(|e| FencingError::Io(e.to_string()))?;
        let mut offset = 0;
        while offset < bytes.len() {
            debit(ctx, bytes.len() - offset)?;
            match source.read(&mut bytes[offset..]) {
                Ok(0) => {
                    return Err(FencingError::Io(
                        std::io::Error::from(std::io::ErrorKind::UnexpectedEof).to_string(),
                    )
                    .into());
                }
                Ok(n) => offset += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(FencingError::Io(e.to_string()).into()),
            }
        }
        debit(ctx, 32)
    })
}
fn validate_request_paid(
    request: &ProtectedEndpointSigningRequestV1,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> ResultV<()> {
    let shared = RefCell::new(ctx);
    validate_protected_request_with(
        request,
        |domain| domain_frame(domain, &(), &mut shared.borrow_mut()),
        |n| debit(&mut shared.borrow_mut(), n),
    )
}
fn same<T: Serialize + PartialEq>(
    left: &T,
    right: &T,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> ResultV<bool> {
    // The original Rust equality is authoritative. Original serde only supplies
    // a measured bound for the closed typed comparison, with all wire paid.
    let a = encode_paid(left, usize::MAX, ctx)?;
    let b = encode_paid(right, usize::MAX, ctx)?;
    debit(ctx, add(a.as_ref().len(), b.as_ref().len())?)?;
    Ok(left == right)
}
struct ProtectedValidation<'a, 'b> {
    ctx: &'a mut FormalBoundedContextV1<'b>,
}
impl ProtectedSignatureValidationOpsV1 for ProtectedValidation<'_, '_> {
    type Error = BoundedAcquireErrorV1;
    fn validate_request(&mut self, r: &ProtectedEndpointSigningRequestV1) -> ResultV<()> {
        validate_request_paid(r, self.ctx)
    }
    fn verify_attestation(
        &mut self,
        b: &ProtectedEndpointSignatureBodyV1,
        s: &str,
        k: &str,
    ) -> ResultV<()> {
        verify_paid(PROTECTED_SIGNATURE_ATTEST_DOMAIN, b, s, k, self.ctx)
    }
    fn instance_digest(&mut self, i: &EndpointInstanceBindingV1) -> ResultV<String> {
        debit(self.ctx, instance_payload(i)?)?;
        i.validate()?;
        hash_paid(i, self.ctx)
    }
    fn request_digest(&mut self, r: &ProtectedEndpointSigningRequestV1) -> ResultV<String> {
        validate_request_paid(r, self.ctx)?;
        hash_paid(r, self.ctx)
    }
    fn authorized(&mut self, p: &TrustedFencingPolicyV1, r: &str) -> ResultV<bool> {
        allowed(p, r, self.ctx)
    }
    fn compare(&mut self, n: usize) -> ResultV<()> {
        debit(self.ctx, n)
    }
    fn verify_endpoint(&mut self, k: &str, s: &str, p: &[u8]) -> ResultV<()> {
        let _hex = reserve(
            self.ctx.memory(),
            add(
                add(mul(s.len() / 2, 2)?.max(8), 32)?,
                2 * size_of::<Vec<u8>>(),
            )?,
        )?;
        verify_endpoint_preimage_with(k, s, p, |n| debit(self.ctx, n))
    }
}
pub(in crate::controlled_authority) struct PaidNativeCheckpointSignatureV1<'a> {
    draft: PaidNativeCheckpointDraftV1<'a>,
    lease: PaidRemoteLeaseV1,
    rpc: ReservedValue<SignedMonotonicOracleRequestV1>,
}
impl std::fmt::Debug for PaidNativeCheckpointSignatureV1<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaidNativeCheckpointSignatureV1")
            .finish_non_exhaustive()
    }
}
impl PaidNativeCheckpointSignatureV1<'_> {
    pub(in crate::controlled_authority) fn signature(&self) -> &ProtectedEndpointSignatureV1 {
        self.lease
            .reply
            .body
            .protected_signature
            .as_ref()
            .expect("private verified signature")
    }
    pub(in crate::controlled_authority) fn endpoint_signature_hex(&self) -> &str {
        &self.signature().body.endpoint_signature
    }
    pub(in crate::controlled_authority) fn endpoint_public_key(&self) -> &str {
        &self.lease.request.body.instance.endpoint_public_key
    }
    pub(in crate::controlled_authority) fn validate_context(
        &self,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> ResultV<()> {
        step(ctx)?;
        if !self.uses_budget(ctx.memory()) || self.lease.identity != ctx.work().identity() {
            return Err(FormalError::Invalid(
                "bounded signed checkpoint foreign pool or meter".into(),
            )
            .into());
        }
        self.draft.validate_context(ctx)?;
        Ok(())
    }
    pub(in crate::controlled_authority) fn draft(&self) -> &PaidNativeCheckpointDraftV1<'_> {
        &self.draft
    }
    pub(in crate::controlled_authority) fn checkpoint(&self) -> &EndpointCheckpointBindingV1 {
        self.lease.checkpoint()
    }
    pub(in crate::controlled_authority) fn native_checkpoint_bytes(&self) -> Option<&[u8]> {
        self.lease.native_checkpoint_bytes()
    }
    pub(in crate::controlled_authority) fn request(&self) -> &ProtectedEndpointSigningRequestV1 {
        match &self.rpc.body.command {
            MonotonicOracleCommandV1::ProtectedSign(request) => request,
            _ => unreachable!("private NativeCheckpoint RPC"),
        }
    }
    pub(in crate::controlled_authority) fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.draft.uses_budget(memory)
            && self.lease.uses_budget(memory)
            && self.rpc.uses_budget(memory)
    }
    pub(in crate::controlled_authority) fn reserved_bytes(&self) -> usize {
        self.draft.reserved_bytes() + self.lease.reserved_bytes() + self.rpc.reserved_bytes()
    }
}
pub(in crate::controlled_authority) fn sign_native_checkpoint_bounded_v1<'a>(
    provider: &RemoteMonotonicEndpointProvider,
    mut lease: PaidRemoteLeaseV1,
    draft: PaidNativeCheckpointDraftV1<'a>,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> ResultV<PaidNativeCheckpointSignatureV1<'a>> {
    step(ctx)?;
    draft.validate_context(ctx)?;
    if !lease.uses_budget(ctx.memory()) || lease.identity != ctx.work().identity() {
        return Err(
            FormalError::Invalid("bounded native sign foreign pool or meter".into()).into(),
        );
    }
    // Four bounded digest temporaries: preflight instance, envelope digest and
    // the original verifier's instance/request digests. Keep their phase paid
    // until every comparison has finished; returned fields have separate fees.
    let _digest_phase = reserve(ctx.memory(), 4 * (64 + size_of::<String>()))?;
    debit(ctx, lease.request.body.authentication_public_key.len())?;
    let mut authentication = [0; 32];
    hex::decode_to_slice(
        &lease.request.body.authentication_public_key,
        &mut authentication,
    )
    .map_err(|e| invalid(e.to_string()))?;
    step(ctx)?;
    if authentication != provider.key.verifying_key().to_bytes() {
        return Err(FencingError::FencingUnproven.into());
    }
    if !same(
        &provider.policy.provider_public_key,
        &lease.policy.provider_public_key,
        ctx,
    )? || !same(
        &provider.policy.authorized_resource_bindings,
        &lease.policy.authorized_resource_bindings,
        ctx,
    )? {
        return Err(FencingError::FencingUnproven.into());
    }
    if !same(draft.before_checkpoint(), lease.checkpoint(), ctx)? {
        return Err(FencingError::FencingUnproven.into());
    }
    let instance = &lease.request.body.instance;
    debit(ctx, instance_payload(instance)?)?;
    instance.validate()?;
    let instance_digest = hash_paid(instance, ctx)?;
    debit(
        ctx,
        add(draft.body().instance_digest.len(), instance_digest.len())?,
    )?;
    if draft.body().instance_digest != instance_digest
        || draft.body().revision
            != draft
                .before_checkpoint()
                .revision
                .checked_add(1)
                .ok_or(FencingError::FencingUnproven)?
    {
        return Err(FencingError::FencingUnproven.into());
    }
    let p = draft.preimage().len();
    let rpc_payload = add(
        add(
            add(instance_payload(instance)?, lease.reply.body.lease.len())?,
            64 + 64 + 128,
        )?,
        add(add(64 + 64, checkpoint_payload(lease.checkpoint())?)?, p)?,
    )?;
    let typed = add(
        add(
            size_of::<PaidNativeCheckpointDraftV1<'a>>(),
            size_of::<PaidRemoteLeaseV1>(),
        )?,
        size_of::<SignedMonotonicOracleRequestV1>(),
    )?;
    let extra = size_of::<PaidNativeCheckpointSignatureV1<'a>>()
        .checked_sub(typed)
        .ok_or_else(layout)?;
    let rpc_layout = add(
        add(
            add(
                size_of::<SignedMonotonicOracleRequestV1>(),
                size_of::<ProtectedEndpointSigningRequestV1>(),
            )?,
            rpc_payload,
        )?,
        extra,
    )?;
    let rpc_fee = ctx
        .memory()
        .reserve(rpc_layout, MemoryClass::Ordinary)
        .map_err(|_| FormalError::Invalid("bounded native sign RPC admission".into()))?;
    let challenge = challenge_paid(ctx)?;
    debit(ctx, checkpoint_payload(lease.checkpoint())?)?;
    let checkpoint = lease.checkpoint().clone();
    if checkpoint.commitment.capacity() > checkpoint.commitment.len()
        || checkpoint.head.decision_hash.capacity() > checkpoint.head.decision_hash.len()
    {
        return Err(layout());
    }
    let mut preimage = Vec::with_capacity(p);
    if preimage.capacity() > p {
        return Err(layout());
    }
    for chunk in draft.preimage().chunks(8192) {
        debit(ctx, chunk.len())?;
        preimage.extend_from_slice(chunk);
    }
    let signing = ProtectedEndpointSigningRequestV1 {
        schema_version: 1,
        instance_digest,
        checkpoint,
        challenge,
        purpose: ProtectedEndpointSignaturePurposeV1::NativeCheckpoint,
        preimage,
    };
    validate_request_paid(&signing, ctx)?;
    step(ctx)?;
    if lease
        .reply
        .body
        .durable_set
        .as_ref()
        .is_some_and(|s| s.activation_digest.is_none())
    {
        return Err(FencingError::FencingUnproven.into());
    }
    if lease.reply.body.protected_custody
        != Some(ProtectedSignerCustodyV1::NonexportableLeaseBoundV1)
        || lease.reply.body.pending.is_some()
        || lease.reply.body.seal.is_some()
    {
        return Err(FencingError::FencingUnproven.into());
    }
    debit(ctx, instance_payload(instance)?)?;
    let copied_instance = instance.clone();
    for s in instance_text(&copied_instance) {
        if s.capacity() > s.len() {
            return Err(layout());
        }
    }
    debit(
        ctx,
        add(
            lease.request.body.authentication_public_key.len(),
            lease.reply.body.lease.len(),
        )?,
    )?;
    let envelope = MonotonicOracleRequestBodyV1 {
        schema_version: 1,
        instance: copied_instance,
        authentication_public_key: text(&lease.request.body.authentication_public_key)?,
        lease: Some(text(&lease.reply.body.lease)?),
        challenge: challenge_paid(ctx)?,
        command: MonotonicOracleCommandV1::ProtectedSign(Box::new(signing)),
    };
    let request_digest = hash_paid(&envelope, ctx)?;
    let signature = sign_paid(REQUEST, &envelope, &provider.key, ctx)?;
    if signature.capacity() > 128 {
        return Err(layout());
    }
    let rpc = rpc_fee.retain(SignedMonotonicOracleRequestV1 {
        body: envelope,
        signature,
    });
    let wire = encode_paid(&*rpc, LIMIT, ctx)?;
    write_all_paid(
        &mut lease._stream,
        &(wire.as_ref().len() as u32).to_be_bytes(),
        ctx,
    )?;
    write_all_paid(&mut lease._stream, wire.as_ref(), ctx)?;
    drop(wire);
    let reply = read_reply_paid(&mut lease._stream, ctx)?;
    validate_reply_with(
        &reply,
        &request_digest,
        &rpc.body.challenge,
        Some(&lease.reply.body.lease),
        instance,
        &lease.policy,
        &mut Validation { ctx },
    )?;
    let state = &reply.body;
    let old = &lease.reply.body;
    if !same(&state.durable_set, &old.durable_set, ctx)?
        || !same(&state.resource_binding, &old.resource_binding, ctx)?
        || !same(&state.protected_custody, &old.protected_custody, ctx)?
        || !same(
            &state.native_checkpoint_bytes,
            &old.native_checkpoint_bytes,
            ctx,
        )?
        || !same(&state.checkpoint, &old.checkpoint, ctx)?
        || state.pending.is_some()
        || !same(&state.seal, &old.seal, ctx)?
    {
        return Err(FencingError::FencingUnproven.into());
    }
    let signature = state
        .protected_signature
        .as_ref()
        .ok_or(FencingError::FencingUnproven)?;
    if !same(
        &signature.body.resource_binding,
        &state.resource_binding,
        ctx,
    )? {
        return Err(FencingError::FencingUnproven.into());
    }
    let MonotonicOracleCommandV1::ProtectedSign(request) = &rpc.body.command else {
        return Err(FencingError::FencingUnproven.into());
    };
    verify_protected_endpoint_signature_with(
        signature,
        request,
        instance,
        &lease.policy,
        &mut ProtectedValidation { ctx },
    )?;
    step(ctx)?;
    // The old reply and fees stay alive throughout all verification. Only a
    // completely verified replacement can become the returned lease state.
    lease.reply = reply;
    Ok(PaidNativeCheckpointSignatureV1 { draft, lease, rpc })
}
#[cfg(test)]
#[path = "fencing_remote_bounded_sign_tests.rs"]
mod tests;

#[path = "fencing_remote_bounded_begin.rs"]
mod bounded_begin;
pub(in crate::controlled_authority) use bounded_begin::{
    PaidNativeCheckpointBeginV1, begin_signed_native_checkpoint_bounded_v1,
};

#[path = "fencing_remote_bounded_initialize.rs"]
mod bounded_initialize;
pub(in crate::controlled_authority) use bounded_initialize::initialize_protected_registration_bounded_v1;
