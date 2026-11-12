//! Frozen receipt signing accepts only a private actual None-Hot source.
use super::*;
use crate::controlled_authority::formal::endpoint::PaidNormalFrozenReceiptSourceV1;
pub(in crate::controlled_authority) struct PaidNormalFrozenReceiptSignatureV1<'a> {
    source: PaidNormalFrozenReceiptSourceV1<'a>,
    lease: PaidRemoteLeaseV1,
    _rpc: ReservedValue<SignedMonotonicOracleRequestV1>,
}
impl PaidNormalFrozenReceiptSignatureV1<'_> {
    pub(in crate::controlled_authority) fn source(&self) -> &PaidNormalFrozenReceiptSourceV1<'_> {
        &self.source
    }
    pub(in crate::controlled_authority) fn endpoint_signature_hex(&self) -> &str {
        &self
            .lease
            .reply
            .body
            .protected_signature
            .as_ref()
            .expect("verified normal Frozen receipt signature")
            .body
            .endpoint_signature
    }
}
impl PaidNormalFrozenReceiptSignatureV1<'_> {
    pub(in crate::controlled_authority) fn into_lease(self) -> PaidRemoteLeaseV1 {
        self.lease
    }
}

pub(in crate::controlled_authority) fn sign_normal_frozen_receipt_bounded_v1<'a>(
    provider: &RemoteMonotonicEndpointProvider,
    mut lease: PaidRemoteLeaseV1,
    source: PaidNormalFrozenReceiptSourceV1<'a>,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> ResultV<PaidNormalFrozenReceiptSignatureV1<'a>> {
    source.validate_context(ctx)?;
    if !lease.uses_budget(ctx.memory()) || lease.identity != ctx.work().identity() {
        return Err(FencingError::FencingUnproven.into());
    }
    if !same(source.instance(), &lease.request.body.instance, ctx)?
        || !same(source.checkpoint(), lease.checkpoint(), ctx)?
    {
        return Err(FencingError::FencingUnproven.into());
    }
    let _digest_fee = reserve(ctx.memory(), 64 + size_of::<String>())?;
    debit(ctx, lease.request.body.authentication_public_key.len())?;
    let mut auth = [0u8; 32];
    hex::decode_to_slice(&lease.request.body.authentication_public_key, &mut auth)
        .map_err(|e| invalid(e.to_string()))?;
    if auth != provider.key.verifying_key().to_bytes()
        || !same(
            &provider.policy.provider_public_key,
            &lease.policy.provider_public_key,
            ctx,
        )?
        || !same(
            &provider.policy.authorized_resource_bindings,
            &lease.policy.authorized_resource_bindings,
            ctx,
        )?
    {
        return Err(FencingError::FencingUnproven.into());
    }
    let instance = &lease.request.body.instance;
    debit(ctx, instance_payload(instance)?)?;
    instance.validate()?;
    let instance_digest = hash_paid(instance, ctx)?;
    debit(
        ctx,
        add(instance_digest.len(), source.instance_digest().len())?,
    )?;
    if instance_digest != source.instance_digest() {
        return Err(FencingError::FencingUnproven.into());
    }
    let p = source.preimage().len();
    let payload = add(
        add(
            add(instance_payload(instance)?, lease.reply.body.lease.len())?,
            64 + 64 + 128,
        )?,
        add(add(64 + 64, checkpoint_payload(lease.checkpoint())?)?, p)?,
    )?;
    let covered = add(
        add(
            size_of::<PaidNormalFrozenReceiptSourceV1<'a>>(),
            size_of::<PaidRemoteLeaseV1>(),
        )?,
        size_of::<SignedMonotonicOracleRequestV1>(),
    )?;
    let extra = size_of::<PaidNormalFrozenReceiptSignatureV1<'a>>()
        .checked_sub(covered)
        .ok_or_else(layout)?;
    let bytes = add(
        add(
            add(
                size_of::<SignedMonotonicOracleRequestV1>(),
                size_of::<ProtectedEndpointSigningRequestV1>(),
            )?,
            payload,
        )?,
        extra,
    )?;
    let reserved = ctx
        .memory()
        .reserve(bytes, MemoryClass::Ordinary)
        .map_err(|_| layout())?;
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
    for chunk in source.preimage().chunks(8192) {
        debit(ctx, chunk.len())?;
        preimage.extend_from_slice(chunk);
    }
    let signing = ProtectedEndpointSigningRequestV1 {
        schema_version: 1,
        instance_digest,
        checkpoint,
        challenge,
        purpose: source.purpose(),
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
        || lease.reply.body.protected_custody
            != Some(ProtectedSignerCustodyV1::NonexportableLeaseBoundV1)
        || lease.reply.body.pending.is_some()
        || lease.reply.body.seal.is_some()
    {
        return Err(FencingError::FencingUnproven.into());
    }
    debit(ctx, instance_payload(instance)?)?;
    let copied = instance.clone();
    for s in instance_text(&copied) {
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
    let body = MonotonicOracleRequestBodyV1 {
        schema_version: 1,
        instance: copied,
        authentication_public_key: text(&lease.request.body.authentication_public_key)?,
        lease: Some(text(&lease.reply.body.lease)?),
        challenge: challenge_paid(ctx)?,
        command: MonotonicOracleCommandV1::ProtectedSign(Box::new(signing)),
    };
    let digest = hash_paid(&body, ctx)?;
    let signature = sign_paid(REQUEST, &body, &provider.key, ctx)?;
    if signature.capacity() > 128 {
        return Err(layout());
    }
    let rpc = reserved.retain(SignedMonotonicOracleRequestV1 { body, signature });
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
        &digest,
        &rpc.body.challenge,
        Some(&lease.reply.body.lease),
        instance,
        &lease.policy,
        &mut Validation { ctx },
    )?;
    validate_protected_result_paid(
        &reply,
        &lease.reply.body,
        &rpc,
        instance,
        &lease.policy,
        ctx,
    )?;
    step(ctx)?;
    lease.reply = reply;
    Ok(PaidNormalFrozenReceiptSignatureV1 {
        source,
        lease,
        _rpc: rpc,
    })
}
