//! Same-FD provider Begin. Pending is not native publication or qualification.
use super::*;
use crate::controlled_authority::formal::PaidSignedNativeCheckpointV1;

struct SignatureEvidence<'a> {
    draft: PaidNativeCheckpointDraftV1<'a>,
    rpc: ReservedValue<SignedMonotonicOracleRequestV1>,
    reply: ReservedValue<SignedMonotonicOracleReplyV1>,
}
struct TransportParts {
    stream: TcpStream,
    request: ReservedValue<SignedMonotonicOracleRequestV1>,
    policy: ReservedValue<TrustedFencingPolicyV1>,
    identity: WorkIdentityV1,
}
pub(in crate::controlled_authority) struct PaidNativeCheckpointBeginV1<'a> {
    evidence: SignatureEvidence<'a>,
    lease: PaidRemoteLeaseV1,
    wire: ReservedValue<Vec<u8>>,
    binding: ReservedValue<EndpointCheckpointBindingV1>,
    source_outer: ReservedValue<()>,
    rpc: ReservedValue<SignedMonotonicOracleRequestV1>,
    outer: ReservedValue<()>,
}
impl std::fmt::Debug for PaidNativeCheckpointBeginV1<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaidNativeCheckpointBeginV1")
            .finish_non_exhaustive()
    }
}
impl PaidNativeCheckpointBeginV1<'_> {
    pub(in crate::controlled_authority) fn intent(&self) -> &CheckpointIntentV1 {
        match &self.rpc.body.command {
            MonotonicOracleCommandV1::Begin(value) => value,
            _ => unreachable!("private Begin request"),
        }
    }
    pub(in crate::controlled_authority) fn pending(&self) -> &CheckpointIntentV1 {
        self.lease.pending().expect("verified Begin pending")
    }
    pub(in crate::controlled_authority) fn checkpoint(&self) -> &EndpointCheckpointBindingV1 {
        self.lease.checkpoint()
    }
    pub(in crate::controlled_authority) fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.evidence.draft.uses_budget(memory)
            && self.evidence.rpc.uses_budget(memory)
            && self.evidence.reply.uses_budget(memory)
            && self.lease.uses_budget(memory)
            && self.wire.uses_budget(memory)
            && self.binding.uses_budget(memory)
            && self.source_outer.uses_budget(memory)
            && self.rpc.uses_budget(memory)
            && self.outer.uses_budget(memory)
    }
    pub(in crate::controlled_authority) fn reserved_bytes(&self) -> usize {
        self.evidence.draft.reserved_bytes()
            + self.evidence.rpc.reserved_bytes()
            + self.evidence.reply.reserved_bytes()
            + self.lease.reserved_bytes()
            + self.wire.reserved_bytes()
            + self.binding.reserved_bytes()
            + self.source_outer.reserved_bytes()
            + self.rpc.reserved_bytes()
            + self.outer.reserved_bytes()
    }
}
struct BeginValidation<'a, 'b> {
    ctx: &'a mut FormalBoundedContextV1<'b>,
}
impl BeginReplyValidationOps for BeginValidation<'_, '_> {
    type Error = BoundedAcquireErrorV1;
    fn same<T: Serialize + PartialEq>(&mut self, a: &T, b: &T) -> ResultV<bool> {
        same(a, b, self.ctx)
    }
}
fn copy_checkpoint(
    source: &EndpointCheckpointBindingV1,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> ResultV<EndpointCheckpointBindingV1> {
    debit(ctx, checkpoint_payload(source)?)?;
    Ok(EndpointCheckpointBindingV1 {
        revision: source.revision,
        commitment: text(&source.commitment)?,
        head: crate::controlled_authority::replicated_protocol::HeadAnchor {
            position: source.head.position,
            decision_hash: text(&source.head.decision_hash)?,
            qualified: source.head.qualified,
        },
    })
}
pub(in crate::controlled_authority) fn begin_signed_native_checkpoint_bounded_v1<'a>(
    provider: &RemoteMonotonicEndpointProvider,
    checkpoint: PaidSignedNativeCheckpointV1<'a>,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> ResultV<PaidNativeCheckpointBeginV1<'a>> {
    checkpoint
        .into_begin_source(ctx)?
        .consume(|signed, wire, binding, source_outer| {
            let PaidNativeCheckpointSignatureV1 {
                draft,
                lease,
                rpc: protected_rpc,
            } = signed;
            let PaidRemoteLeaseV1 {
                _stream,
                request,
                reply: protected_reply,
                policy,
                identity,
            } = lease;
            let mut transport = TransportParts {
                stream: _stream,
                request,
                policy,
                identity,
            };
            let evidence = SignatureEvidence {
                draft,
                rpc: protected_rpc,
                reply: protected_reply,
            };
            step(ctx)?;
            let _digest_phase = reserve(ctx.memory(), 64 + size_of::<String>())?;
            debit(ctx, transport.request.body.authentication_public_key.len())?;
            let mut authentication = [0; 32];
            hex::decode_to_slice(
                &transport.request.body.authentication_public_key,
                &mut authentication,
            )
            .map_err(|e| invalid(e.to_string()))?;
            if authentication != provider.key.verifying_key().to_bytes()
                || !same(
                    &provider.policy.provider_public_key,
                    &transport.policy.provider_public_key,
                    ctx,
                )?
                || !same(
                    &provider.policy.authorized_resource_bindings,
                    &transport.policy.authorized_resource_bindings,
                    ctx,
                )?
            {
                return Err(FencingError::FencingUnproven.into());
            }
            if !same(
                evidence.draft.before_checkpoint(),
                &evidence.reply.body.checkpoint,
                ctx,
            )? {
                return Err(FencingError::FencingUnproven.into());
            }
            validate_begin_gate_with(&evidence.reply.body, |n| debit(ctx, n))?;
            let instance = &transport.request.body.instance;
            // Actual seal serialization is only a measured payload bound; the
            // original seal clone and original Begin schema remain authoritative.
            let seal_wire = encode_paid(&evidence.draft.body().seal, usize::MAX, ctx)?;
            let seal_layout = add(size_of::<EpochSealBodyV1>(), seal_wire.len())?;
            let payload = add(
                add(
                    checkpoint_payload(&evidence.reply.body.checkpoint)?,
                    checkpoint_payload(&binding)?,
                )?,
                add(
                    add(wire.len(), seal_layout)?,
                    add(
                        instance_payload(instance)?,
                        add(
                            add(
                                transport.request.body.authentication_public_key.len(),
                                evidence.reply.body.lease.len(),
                            )?,
                            64 + 128,
                        )?,
                    )?,
                )?,
            )?;
            let rpc_layout = add(
                add(
                    size_of::<SignedMonotonicOracleRequestV1>(),
                    size_of::<CheckpointIntentV1>(),
                )?,
                payload,
            )?;
            let rpc_fee = reserve(ctx.memory(), rpc_layout)?;
            let covered = add(
                add(
                    size_of::<PaidSignedNativeCheckpointV1<'a>>(),
                    size_of::<SignedMonotonicOracleRequestV1>(),
                )?,
                size_of::<SignedMonotonicOracleReplyV1>(),
            )?;
            // Existing owner phases remain held; only positive shell growth is new.
            let outer_fee = reserve(
                ctx.memory(),
                size_of::<PaidNativeCheckpointBeginV1<'a>>().saturating_sub(covered),
            )?;
            let previous = copy_checkpoint(&evidence.reply.body.checkpoint, ctx)?;
            let next = copy_checkpoint(&binding, ctx)?;
            let mut bytes = Vec::with_capacity(wire.len());
            if bytes.capacity() > wire.len() {
                return Err(layout());
            }
            for chunk in wire.chunks(8192) {
                debit(ctx, chunk.len())?;
                bytes.extend_from_slice(chunk);
            }
            debit(ctx, seal_wire.len())?;
            let seal = evidence.draft.body().seal.clone();
            drop(seal_wire);
            let intent = CheckpointIntentV1 {
                previous,
                next,
                checkpoint_bytes: bytes,
                seal,
            };
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
                    transport.request.body.authentication_public_key.len(),
                    evidence.reply.body.lease.len(),
                )?,
            )?;
            let envelope = MonotonicOracleRequestBodyV1 {
                schema_version: 1,
                instance: copied_instance,
                authentication_public_key: text(&transport.request.body.authentication_public_key)?,
                lease: Some(text(&evidence.reply.body.lease)?),
                challenge: challenge_paid(ctx)?,
                command: MonotonicOracleCommandV1::Begin(Box::new(intent)),
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
            let encoded = encode_paid(&*rpc, LIMIT, ctx)?;
            write_all_paid(
                &mut transport.stream,
                &(encoded.len() as u32).to_be_bytes(),
                ctx,
            )?;
            write_all_paid(&mut transport.stream, &encoded, ctx)?;
            drop(encoded);
            let reply = read_reply_paid(&mut transport.stream, ctx)?;
            validate_reply_with(
                &reply,
                &request_digest,
                &rpc.body.challenge,
                Some(&evidence.reply.body.lease),
                instance,
                &transport.policy,
                &mut Validation { ctx },
            )?;
            let MonotonicOracleCommandV1::Begin(intent) = &rpc.body.command else {
                unreachable!("private Begin")
            };
            validate_begin_reply_with(
                &reply.body,
                &evidence.reply.body,
                intent,
                &mut BeginValidation { ctx },
            )?;
            let lease = PaidRemoteLeaseV1 {
                _stream: transport.stream,
                request: transport.request,
                reply,
                policy: transport.policy,
                identity: transport.identity,
            };
            Ok(PaidNativeCheckpointBeginV1 {
                evidence,
                lease,
                wire,
                binding,
                source_outer,
                rpc,
                outer: outer_fee.retain(()),
            })
        })
}
#[cfg(test)]
#[path = "fencing_remote_bounded_begin_tests.rs"]
mod tests;
