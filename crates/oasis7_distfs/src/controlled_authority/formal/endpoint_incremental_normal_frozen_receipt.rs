//! Frozen receipt source is minted only after exact None has durably finished.
use super::*;
pub(in crate::controlled_authority) struct PaidNormalFrozenReceiptSourceV1<'a> {
    body: ReservedValue<FormalFrozenReceiptBodyV1>,
    instance: &'a fencing::EndpointInstanceBindingV1,
    before: fencing::EndpointCheckpointBindingV1,
    before_fee: ReservedValue<()>,
    instance_digest: ReservedValue<String>,
    preimage: ReservedValue<Vec<u8>>,
    identity: oasis7_formal_protocol::controlled_authority::formal::bounded_context::WorkIdentityV1,
    _outer: ReservedValue<()>,
}
impl PaidNormalFrozenReceiptSourceV1<'_> {
    pub(in crate::controlled_authority) fn purpose(
        &self,
    ) -> fencing::ProtectedEndpointSignaturePurposeV1 {
        fencing::ProtectedEndpointSignaturePurposeV1::FrozenReceipt
    }
    pub(in crate::controlled_authority) fn instance(&self) -> &fencing::EndpointInstanceBindingV1 {
        self.instance
    }
    pub(in crate::controlled_authority) fn checkpoint(
        &self,
    ) -> &fencing::EndpointCheckpointBindingV1 {
        &self.before
    }
    pub(in crate::controlled_authority) fn instance_digest(&self) -> &str {
        &self.instance_digest
    }
    pub(in crate::controlled_authority) fn preimage(&self) -> &[u8] {
        &self.preimage
    }
    pub(in crate::controlled_authority) fn validate_context(
        &self,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        ctx.consume_steps(1)?;
        if self.identity != ctx.work().identity()
            || !self.body.uses_budget(ctx.memory())
            || !self.before_fee.uses_budget(ctx.memory())
            || !self.instance_digest.uses_budget(ctx.memory())
            || !self.preimage.uses_budget(ctx.memory())
            || !self._outer.uses_budget(ctx.memory())
        {
            return Err(invalid("normal Frozen receipt pool/Core"));
        }
        Ok(())
    }
    pub(super) fn receipt(
        &self,
        signature: &str,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<ReservedValue<FormalFrozenReceiptV1>, FormalError> {
        self.validate_context(ctx)?;
        let n = self
            .body
            .authority_id
            .len()
            .checked_add(self.body.instance_digest.len())
            .and_then(|n| n.checked_add(self.body.reservation_id.len()))
            .and_then(|n| n.checked_add(signature.len()))
            .and_then(|n| n.checked_add(size_of::<FormalFrozenReceiptV1>()))
            .ok_or_else(|| invalid("normal Frozen receipt layout"))?;
        let fee = ctx
            .memory()
            .reserve(n, MemoryClass::Ordinary)
            .map_err(|_| invalid("normal Frozen receipt admission"))?;
        ctx.charge(1, n as u64)?;
        Ok(fee.retain(FormalFrozenReceiptV1 {
            body: (*self.body).clone(),
            signature_hex: signature.to_owned(),
        }))
    }
}
pub(super) fn source<'a>(
    endpoint: &'a FileFormalEndpoint,
    lease: &PaidRemoteLeaseV1,
    reservation: &VerifiedFormalReservationV1,
    expected_none: &store::bounded_request::PaidRequestStateV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PaidNormalFrozenReceiptSourceV1<'a>, FormalError> {
    let request = &reservation.body().request_digest;
    let actual = endpoint
        .store
        .read_request_bounded(request, memory, work, stop)?
        .ok_or_else(|| invalid("normal Frozen None was not published"))?;
    if !store::bounded_transaction::same(actual.state(), expected_none.state(), memory, work, stop)?
    {
        return Err(invalid("normal Frozen None durable source changed"));
    }
    if !matches!(
        actual.state(),
        FormalRequestStateV1::Frozen {
            receipt: None,
            certificate: None,
            ..
        }
    ) {
        return Err(invalid("normal Frozen receipt requires durable None"));
    }
    if !store::bounded_transaction::same(
        lease.checkpoint(),
        &endpoint.store.checkpoint,
        memory,
        work,
        stop,
    )? {
        return Err(invalid("normal Frozen receipt checkpoint changed"));
    }
    let instance = match endpoint.role {
        EndpointRole::Primary => &endpoint.authority.definition().primary.instance,
        EndpointRole::Replica => &endpoint.authority.definition().replica.instance,
    };
    let instance_digest = store::bounded_reopen::hash_encoded(instance, None, memory, work, stop)?;
    let (before, before_fee) = store::bounded_transaction::copy_checkpoint(
        &endpoint.store.checkpoint,
        memory,
        work,
        stop,
    )?;
    let mut ctx = work.bounded_context(memory, stop);
    let authority_id = normal_frozen_hash_bounded(
        oasis7_formal_protocol::controlled_authority::formal::AUTHORITY_ID_DOMAIN_V1,
        endpoint.authority.definition(),
        &mut ctx,
    )?;
    let reservation_id = normal_frozen_hash_bounded(
        "oasis7.formal.frozen_identity.v1",
        reservation.body(),
        &mut ctx,
    )?;
    let body_fee = memory
        .reserve(
            size_of::<FormalFrozenReceiptBodyV1>() + 3 * 64,
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("normal Frozen receipt body admission"))?;
    ctx.charge(1, 3 * 64)?;
    let body = body_fee.retain(FormalFrozenReceiptBodyV1 {
        authority_id: (*authority_id).clone(),
        role: endpoint.role,
        instance_digest: (*instance_digest).clone(),
        reservation_id: (*reservation_id).clone(),
    });
    let wire = store::context_wire::encode_paid(
        &*body,
        64 * 1024,
        &mut ctx,
        "Frozen receipt bound",
        "Frozen receipt body",
    )?;
    let domain = fencing::ProtectedEndpointSignaturePurposeV1::FrozenReceipt.domain();
    let n = wire
        .len()
        .checked_add(domain.len())
        .and_then(|n| n.checked_add(8))
        .ok_or_else(|| invalid("normal Frozen receipt frame layout"))?;
    if n > 64 * 1024 {
        return Err(invalid("normal Frozen receipt frame bound"));
    }
    let fee = memory
        .reserve(n + size_of::<Vec<u8>>(), MemoryClass::Ordinary)
        .map_err(|_| invalid("normal Frozen receipt frame admission"))?;
    let mut preimage = Vec::with_capacity(n);
    if preimage.capacity() > n {
        return Err(invalid("normal Frozen receipt frame capacity"));
    }
    for segment in [
        &(domain.len() as u64).to_be_bytes()[..],
        domain.as_bytes(),
        wire.as_slice(),
    ] {
        for chunk in segment.chunks(8192) {
            ctx.charge(1, chunk.len() as u64)?;
            preimage.extend_from_slice(chunk);
        }
    }
    let outer = memory
        .reserve(
            size_of::<PaidNormalFrozenReceiptSourceV1<'a>>(),
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("normal Frozen receipt owner admission"))?;
    ctx.consume_steps(1)?;
    Ok(PaidNormalFrozenReceiptSourceV1 {
        body,
        instance,
        before,
        before_fee,
        instance_digest,
        preimage: fee.retain(preimage),
        identity: ctx.work().identity(),
        _outer: outer.retain(()),
    })
}
