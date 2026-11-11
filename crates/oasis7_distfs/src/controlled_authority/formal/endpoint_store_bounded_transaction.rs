//! Exclusive, budgeted native publication and disk-driven unknown-result recovery.
//! Only private retained request updates enter this path; Qualified remains rejected.
use super::super::FormalArchiveReadWorkV1;
use super::*;
use crate::controlled_authority::fencing::{
    BoundedAcquireErrorV1, EndpointInstanceBindingV1, RemoteMonotonicEndpointProvider,
    TrustedFencingPolicyV1, acquire_provider_lease_bounded_v1,
    begin_signed_native_checkpoint_bounded_v1, sign_native_checkpoint_bounded_v1,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::mem::size_of;
type ResultV<T> = Result<T, BoundedAcquireErrorV1>;
fn same<T: Serialize + PartialEq>(
    a: &T,
    b: &T,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<bool, FormalError> {
    let mut ctx = work.bounded_context(memory, stop);
    let left = super::context_wire::encode_paid(
        a,
        usize::MAX,
        &mut ctx,
        "bounded transaction comparison wire",
        "bounded transaction comparison encoding",
    )?;
    let right = super::context_wire::encode_paid(
        b,
        usize::MAX,
        &mut ctx,
        "bounded transaction comparison wire",
        "bounded transaction comparison encoding",
    )?;
    let n = left
        .len()
        .checked_add(right.len())
        .ok_or_else(|| invalid("bounded transaction comparison layout"))?;
    ctx.charge(1, n as u64)?;
    Ok(a == b)
}
fn fee(
    memory: &MemoryBudget,
    n: usize,
) -> Result<oasis7_proto::memory_budget::MemoryReservation, FormalError> {
    memory
        .reserve(n, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded native transaction admission"))
}
fn copy_checkpoint(
    source: &fencing::EndpointCheckpointBindingV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<(fencing::EndpointCheckpointBindingV1, ReservedValue<()>), FormalError> {
    let n = size_of::<fencing::EndpointCheckpointBindingV1>()
        .checked_add(source.commitment.len())
        .and_then(|n| n.checked_add(source.head.decision_hash.len()))
        .ok_or_else(|| invalid("bounded cache checkpoint layout"))?;
    let reservation = fee(memory, n)?;
    work.charge(1, n as u64, stop)?;
    let value = source.clone();
    if value.commitment.capacity() > source.commitment.len()
        || value.head.decision_hash.capacity() > source.head.decision_hash.len()
    {
        return Err(invalid("bounded cache checkpoint capacity"));
    }
    Ok((value, reservation.retain(())))
}
fn copy_body(
    source: &HotBody,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<(HotBody, ReservedValue<()>), FormalError> {
    let mut ctx = work.bounded_context(memory, stop);
    let (layout, payload) = super::bounded_native_draft::body_layout(source, &mut ctx)?;
    let reservation = fee(memory, layout)?;
    ctx.charge(1, payload as u64)?;
    let body = source.clone();
    super::bounded_native_draft::visit_body(&body, &mut |s| {
        if s.capacity() > s.len() {
            return Err(invalid("bounded cache clone capacity"));
        }
        Ok(())
    })?;
    Ok((body, reservation.retain(())))
}
fn disk_binding(
    source: &PaidNativeHotV1,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<(fencing::EndpointCheckpointBindingV1, ReservedValue<()>), FormalError> {
    let head = source
        .body()
        .decision_head
        .as_ref()
        .unwrap_or(&source.body().head);
    let layout = size_of::<fencing::EndpointCheckpointBindingV1>()
        .checked_add(64)
        .and_then(|n| n.checked_add(head.record_id.len()))
        .ok_or_else(|| invalid("bounded disk checkpoint layout"))?;
    let reservation = fee(memory, layout)?;
    let mut hash = blake3::Hasher::new();
    for chunk in source.bytes().chunks(8192) {
        work.charge(1, chunk.len() as u64, stop)?;
        hash.update(chunk);
    }
    work.charge(
        1,
        head.record_id
            .len()
            .checked_add(64)
            .ok_or_else(|| invalid("bounded disk checkpoint layout"))? as u64,
        stop,
    )?;
    let commitment = hash.finalize().to_hex().to_string();
    let decision_hash = head.record_id.clone();
    if commitment.capacity() > 64 || decision_hash.capacity() > head.record_id.len() {
        return Err(invalid("bounded disk checkpoint capacity"));
    }
    Ok((
        fencing::EndpointCheckpointBindingV1 {
            revision: source.body().revision,
            commitment,
            head: crate::controlled_authority::replicated_protocol::HeadAnchor {
                position: head.position,
                decision_hash,
                qualified: source.body().decision_head.is_none(),
            },
        },
        reservation.retain(()),
    ))
}
impl Store {
    pub(super) fn publish_pending_update_bounded(
        &mut self,
        update: PreparedPendingRequestUpdate,
        provider: &RemoteMonotonicEndpointProvider,
        instance: &EndpointInstanceBindingV1,
        policy: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> ResultV<()> {
        let draft = self.prepare_pending_native_checkpoint_draft_from_disk_bounded(
            update,
            &instance.endpoint_public_key,
            memory,
            work,
            stop,
        )?;
        let mut ctx = work.bounded_context(memory, stop);
        let lease = acquire_provider_lease_bounded_v1(
            provider,
            instance,
            &self.checkpoint,
            policy,
            &mut ctx,
        )?;
        let signed = sign_native_checkpoint_bounded_v1(provider, lease, draft, &mut ctx)?;
        let checkpoint =
            super::bounded_signed_checkpoint::prepare_signed_native_checkpoint_bounded_v1(
                signed, &mut ctx,
            )?;
        let begun = begin_signed_native_checkpoint_bounded_v1(provider, checkpoint, &mut ctx)?;
        // Prepay and construct BOTH cache replacements before native publication.
        let (body, body_fee) = begun.draft().paid_cache_copy(&mut ctx)?;
        drop(ctx);
        let (binding, binding_fee) = copy_checkpoint(begun.next_checkpoint(), memory, work, stop)?;
        #[cfg(test)]
        if self.fault == Some(1) {
            return Err(FormalError::Io("injected after provider intent".into()).into());
        }
        self.atomic_hot_bounded(begun.native_bytes(), memory, work, stop)?;
        #[cfg(test)]
        if self.fault == Some(2) {
            return Err(FormalError::Io("injected after native checkpoint".into()).into());
        }
        let mut ctx = work.bounded_context(memory, stop);
        let finished = begun.finish_bounded(provider, &mut ctx)?;
        drop(finished); // Ends the genuine Draft's Store borrow before cache mutation.
        self.body = body;
        self.checkpoint = binding;
        self._bounded_cache_fee = Some(body_fee);
        self._bounded_checkpoint_fee = Some(binding_fee);
        Ok(())
    }
    /// Every attempt acquires a NEW real FD and reads actual disk Hot. It never
    /// reuses an owner whose Begin/Finish result was unknown.
    pub(super) fn reconcile_pending_update_bounded(
        &mut self,
        provider: &RemoteMonotonicEndpointProvider,
        instance: &EndpointInstanceBindingV1,
        policy: &TrustedFencingPolicyV1,
        memory: &MemoryBudget,
        work: &FormalArchiveReadWorkV1,
        stop: &mut dyn FnMut() -> bool,
    ) -> ResultV<()> {
        let disk = self.read_hot_bounded(&instance.endpoint_public_key, memory, work, stop)?;
        let (actual, _actual_fee) = disk_binding(&disk, memory, work, stop)?;
        let mut ctx = work.bounded_context(memory, stop);
        let lease = acquire_provider_lease_bounded_v1(
            provider,
            instance,
            &self.checkpoint,
            policy,
            &mut ctx,
        )?;
        drop(ctx);
        if let Some(intent) = lease.pending() {
            if !same(&actual, &intent.previous, memory, work, stop)?
                && !same(&actual, &intent.next, memory, work, stop)?
            {
                return Err(FormalError::Rollback(
                    "formal checkpoint outside monotonic intent".into(),
                )
                .into());
            }
            // The provider payload is data until the SAME original Hot codec,
            // typed signature checks and complete reference validator accept it.
            let n = intent.checkpoint_bytes.len();
            if n > LIMIT {
                return Err(invalid("formal storage byte limit").into());
            }
            let reservation = fee(
                memory,
                n.checked_add(size_of::<Vec<u8>>())
                    .ok_or_else(|| invalid("bounded recovery wire layout"))?,
            )?;
            let mut bytes = Vec::with_capacity(n);
            if bytes.capacity() > n {
                return Err(invalid("bounded recovery wire capacity").into());
            }
            for chunk in intent.checkpoint_bytes.chunks(8192) {
                work.charge(1, chunk.len() as u64, stop)?;
                bytes.extend_from_slice(chunk);
            }
            let path_fee = fee(memory, size_of::<ReservedValue<()>>())?.retain(());
            let next = super::bounded_hot_read::decode_hot_paid_wire(
                reservation.retain(bytes),
                path_fee,
                &instance.endpoint_public_key,
                memory,
                work,
                stop,
            )?;
            let (binding, binding_fee) = disk_binding(&next, memory, work, stop)?;
            if !same(&binding, &intent.next, memory, work, stop)?
                || !same(
                    &next.body().instance_digest,
                    &self.body.instance_digest,
                    memory,
                    work,
                    stop,
                )?
                || !same(
                    &next.body().authority_id,
                    &self.body.authority_id,
                    memory,
                    work,
                    stop,
                )?
                || Some(next.body().revision) != intent.previous.revision.checked_add(1)
                || !same(
                    &next.body().seal.as_ref(),
                    &intent.seal.as_ref().or(self.body.seal.as_ref()),
                    memory,
                    work,
                    stop,
                )?
            {
                return Err(invalid("formal external checkpoint intent binding").into());
            }
            super::bounded_native_refs::validate_recovery_refs(
                self,
                next.body(),
                memory,
                work,
                stop,
            )?;
            let (body, body_fee) = copy_body(next.body(), memory, work, stop)?;
            // Rewrite even an already-next disk snapshot: atomic's fsync and
            // directory sync complete the same durable before-Finish contract.
            self.atomic_hot_bounded(next.bytes(), memory, work, stop)?;
            let mut ctx = work.bounded_context(memory, stop);
            let lease = lease.finish_pending_bounded(provider, &mut ctx)?;
            drop(lease);
            self.body = body;
            self.checkpoint = binding;
            self._bounded_cache_fee = Some(body_fee);
            self._bounded_checkpoint_fee = Some(binding_fee);
        } else {
            if !same(&actual, lease.checkpoint(), memory, work, stop)?
                || !same(
                    &lease.seal(),
                    &disk.body().seal.as_ref(),
                    memory,
                    work,
                    stop,
                )?
            {
                return Err(FormalError::Rollback(
                    "formal monotonic checkpoint/seal mismatch".into(),
                )
                .into());
            }
            super::bounded_native_refs::validate_recovery_refs(
                self,
                disk.body(),
                memory,
                work,
                stop,
            )?;
            let (body, body_fee) = copy_body(disk.body(), memory, work, stop)?;
            let (binding, binding_fee) = copy_checkpoint(&actual, memory, work, stop)?;
            self.body = body;
            self.checkpoint = binding;
            self._bounded_cache_fee = Some(body_fee);
            self._bounded_checkpoint_fee = Some(binding_fee);
        }
        Ok(())
    }
}
#[cfg(test)]
#[path = "endpoint_store_bounded_transaction_tests.rs"]
mod tests;
