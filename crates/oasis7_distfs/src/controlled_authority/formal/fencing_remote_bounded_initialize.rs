//! Original InitializeProtected RPC with authority-derived paid registration.
//! Returning this private lease does not weaken the original Acquire gate.
use super::*;
use crate::controlled_authority::formal::endpoint::PaidColdRegistrationV1;
pub(in crate::controlled_authority) struct PaidProtectedRegistrationReplyV1 {
    _stream: TcpStream,
    _request: ReservedValue<SignedMonotonicOracleRequestV1>,
    reply: ReservedValue<SignedMonotonicOracleReplyV1>,
    _policy: ReservedValue<TrustedFencingPolicyV1>,
    _identity: WorkIdentityV1,
}
impl PaidProtectedRegistrationReplyV1 {
    pub(in crate::controlled_authority) fn checkpoint(&self) -> &EndpointCheckpointBindingV1 {
        &self.reply.body.checkpoint
    }
    pub(in crate::controlled_authority) fn native_checkpoint_bytes(&self) -> Option<&[u8]> {
        self.reply.body.native_checkpoint_bytes.as_deref()
    }
    pub(in crate::controlled_authority) fn seal(&self) -> Option<&EpochSealBodyV1> {
        self.reply.body.seal.as_ref()
    }
}
pub(in crate::controlled_authority) fn initialize_protected_registration_bounded_v1(
    provider: &RemoteMonotonicEndpointProvider,
    source: PaidColdRegistrationV1<'_>,
    pinned: &TrustedFencingPolicyV1,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> ResultV<PaidProtectedRegistrationReplyV1> {
    source.validate(ctx)?;
    #[cfg(not(unix))]
    return Err(FencingError::FencingUnproven.into());
    #[cfg(unix)]
    {
        if !same(
            &provider.policy.provider_public_key,
            &pinned.provider_public_key,
            ctx,
        )? || !same(
            &provider.policy.authorized_resource_bindings,
            &pinned.authorized_resource_bindings,
            ctx,
        )? {
            return Err(FencingError::FencingUnproven.into());
        }
        let instance = source.instance();
        let registration = source.value();
        debit(ctx, registration.authority_id.len())?;
        canonical_hash(&registration.authority_id)?;
        debit(ctx, registration.activation_ref.len())?;
        canonical_hash(&registration.activation_ref)?;
        if registration.unsigned_checkpoint_preimage.len() > 64 * 1024
            || registration.existing_checkpoint.is_some()
        {
            return Err(FencingError::FencingUnproven.into());
        }
        let payload = add(
            add(instance_payload(instance)?, 64 + 64 + 128)?,
            add(
                add(
                    registration.authority_id.len(),
                    registration.activation_ref.len(),
                )?,
                registration.unsigned_checkpoint_preimage.len(),
            )?,
        )?;
        let fee = ctx
            .memory()
            .reserve(
                add(
                    add(
                        size_of::<SignedMonotonicOracleRequestV1>(),
                        size_of::<ProtectedEndpointRegistrationV1>(),
                    )?,
                    payload,
                )?,
                MemoryClass::Ordinary,
            )
            .map_err(|_| layout())?;
        debit(ctx, add(instance.endpoint_public_key.len(), 64)?)?;
        let auth = hex::encode(provider.key.verifying_key().to_bytes());
        if auth == instance.endpoint_public_key {
            return Err(FencingError::FencingUnproven.into());
        }
        step(ctx)?;
        let mut stream = TcpStream::connect_timeout(&provider.address, provider.timeout)
            .map_err(|e| FencingError::Io(e.to_string()))?;
        step(ctx)?;
        stream
            .set_nonblocking(false)
            .map_err(|e| FencingError::Io(e.to_string()))?;
        step(ctx)?;
        stream
            .set_nodelay(true)
            .map_err(|e| FencingError::Io(e.to_string()))?;
        step(ctx)?;
        stream
            .set_read_timeout(Some(provider.timeout))
            .map_err(|e| FencingError::Io(e.to_string()))?;
        step(ctx)?;
        stream
            .set_write_timeout(Some(provider.timeout))
            .map_err(|e| FencingError::Io(e.to_string()))?;
        let challenge = challenge_paid(ctx)?;
        debit(ctx, instance_payload(instance)?)?;
        let copied_instance = instance.clone();
        for value in instance_text(&copied_instance) {
            if value.capacity() > value.len() {
                return Err(layout());
            }
        }
        let n = registration.unsigned_checkpoint_preimage.len();
        let mut preimage = Vec::with_capacity(n);
        if preimage.capacity() > n {
            return Err(layout());
        }
        for chunk in registration.unsigned_checkpoint_preimage.chunks(8192) {
            debit(ctx, chunk.len())?;
            preimage.extend_from_slice(chunk);
        }
        debit(
            ctx,
            add(
                registration.authority_id.len(),
                registration.activation_ref.len(),
            )?,
        )?;
        let registration = ProtectedEndpointRegistrationV1 {
            authority_id: text(&registration.authority_id)?,
            activation_ref: text(&registration.activation_ref)?,
            unsigned_checkpoint_preimage: preimage,
            existing_checkpoint: None,
        };
        step(ctx)?;
        let command = MonotonicOracleCommandV1::InitializeProtected(Box::new(registration));
        let body = MonotonicOracleRequestBodyV1 {
            schema_version: 1,
            instance: copied_instance,
            authentication_public_key: auth,
            lease: None,
            challenge,
            command,
        };
        let _digest = reserve(ctx.memory(), 64 + size_of::<String>())?;
        let request_digest = hash_paid(&body, ctx)?;
        let signature = sign_paid(REQUEST, &body, &provider.key, ctx)?;
        if signature.capacity() > 128 {
            return Err(layout());
        }
        let request = fee.retain(SignedMonotonicOracleRequestV1 { body, signature });
        let wire = encode_paid(&*request, LIMIT, ctx)?;
        write_all_paid(
            &mut stream,
            &(wire.as_ref().len() as u32).to_be_bytes(),
            ctx,
        )?;
        write_all_paid(&mut stream, wire.as_ref(), ctx)?;
        drop(wire);
        let reply = read_reply_paid(&mut stream, ctx)?;
        validate_reply_with(
            &reply,
            &request_digest,
            &request.body.challenge,
            None,
            instance,
            pinned,
            &mut Validation { ctx },
        )?;
        step(ctx)?;
        if reply.body.protected_custody != Some(ProtectedSignerCustodyV1::NonexportableLeaseBoundV1)
            || reply.body.native_checkpoint_bytes.is_none()
        {
            return Err(FencingError::FencingUnproven.into());
        }
        let policy = copy_policy_paid(pinned, ctx)?;
        // Same closed owner shell as Acquire, but a distinct non-Acquire type.
        // read_reply_paid's retained shell accounting therefore stays exact.
        assert_eq!(
            size_of::<PaidProtectedRegistrationReplyV1>(),
            size_of::<PaidRemoteLeaseV1>()
        );
        Ok(PaidProtectedRegistrationReplyV1 {
            _stream: stream,
            _request: request,
            reply,
            _policy: policy,
            _identity: ctx.work().identity(),
        })
    }
}
