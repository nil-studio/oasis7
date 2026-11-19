//! Same-FD Finish. Every error consumes the transaction owner and closes its FD.
use super::*;
pub(super) fn finish_lease_bounded(
    lease: &mut PaidRemoteLeaseV1,
    provider: &RemoteMonotonicEndpointProvider,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> ResultV<()> {
    step(ctx)?;
    if lease.identity != ctx.work().identity() || !lease.uses_budget(ctx.memory()) {
        return Err(FencingError::FencingUnproven.into());
    }
    let _digest_fee = reserve(ctx.memory(), 64 + size_of::<String>())?;
    let old = &lease.reply.body;
    let intent = old
        .pending
        .as_ref()
        .ok_or_else(|| invalid("oracle finish without pending intent"))?;
    debit(ctx, lease.request.body.authentication_public_key.len())?;
    let mut key = [0u8; 32];
    hex::decode_to_slice(&lease.request.body.authentication_public_key, &mut key)
        .map_err(|e| invalid(e.to_string()))?;
    if key != provider.key.verifying_key().to_bytes()
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
    let payload = add(
        instance_payload(&lease.request.body.instance)?,
        add(
            lease.request.body.authentication_public_key.len(),
            add(old.lease.len(), 64 + 128)?,
        )?,
    )?;
    let fee = ctx
        .memory()
        .reserve(
            add(size_of::<SignedMonotonicOracleRequestV1>(), payload)?,
            MemoryClass::Ordinary,
        )
        .map_err(|_| layout())?;
    debit(ctx, instance_payload(&lease.request.body.instance)?)?;
    let instance = lease.request.body.instance.clone();
    for s in instance_text(&instance) {
        if s.capacity() > s.len() {
            return Err(layout());
        }
    }
    debit(
        ctx,
        add(
            lease.request.body.authentication_public_key.len(),
            old.lease.len(),
        )?,
    )?;
    let body = MonotonicOracleRequestBodyV1 {
        schema_version: 1,
        instance,
        authentication_public_key: text(&lease.request.body.authentication_public_key)?,
        lease: Some(text(&old.lease)?),
        challenge: challenge_paid(ctx)?,
        command: MonotonicOracleCommandV1::Finish,
    };
    let digest = hash_paid(&body, ctx)?;
    let signature = sign_paid(REQUEST, &body, &provider.key, ctx)?;
    let rpc = fee.retain(SignedMonotonicOracleRequestV1 { body, signature });
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
        Some(&old.lease),
        &lease.request.body.instance,
        &lease.policy,
        &mut Validation { ctx },
    )?;
    validate_finish_reply_with(&reply.body, old, intent, &mut BeginValidation { ctx })?;
    lease.reply = reply;
    Ok(())
}

impl PaidRemoteLeaseV1 {
    pub(in crate::controlled_authority) fn finish_pending_bounded(
        mut self,
        provider: &RemoteMonotonicEndpointProvider,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> ResultV<Self> {
        finish_lease_bounded(&mut self, provider, ctx)?;
        Ok(self)
    }
}
