//! Paid current native metadata signing data. No Hot publication or execution grant.
use super::*;
use crate::controlled_authority::replicated_protocol::ArtifactRole;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::{
    FormalBoundedContextV1, WorkIdentityV1,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{
    mem::{MaybeUninit, align_of, size_of},
    ptr::NonNull,
};

const PREIMAGE_LIMIT: usize = 64 * 1024;
fn invalid_draft(message: &str) -> FormalError {
    invalid(message)
}
fn add(a: usize, b: usize) -> Result<usize, FormalError> {
    a.checked_add(b)
        .ok_or_else(|| invalid_draft("bounded native draft layout overflow"))
}
fn mul(a: usize, b: usize) -> Result<usize, FormalError> {
    a.checked_mul(b)
        .ok_or_else(|| invalid_draft("bounded native draft layout overflow"))
}
fn debit(ctx: &mut FormalBoundedContextV1<'_>, n: usize) -> Result<(), FormalError> {
    ctx.consume_read_bytes(
        u64::try_from(n).map_err(|_| invalid_draft("bounded native draft byte layout"))?,
    )
}
fn optional(
    value: Option<&String>,
    visit: &mut impl FnMut(&String) -> Result<(), FormalError>,
) -> Result<(), FormalError> {
    if let Some(s) = value {
        visit(s)?;
    }
    Ok(())
}
fn visit_instance(
    value: &fencing::EndpointInstanceBindingV1,
    visit: &mut impl FnMut(&String) -> Result<(), FormalError>,
) -> Result<(), FormalError> {
    for s in [
        &value.world_id,
        &value.chain_id,
        &value.genesis_digest,
        &value.endpoint_id,
        &value.endpoint_public_key,
        &value.instance_id,
        &value.storage_id,
        &value.storage_fingerprint,
    ] {
        visit(s)?;
    }
    Ok(())
}
fn visit_checkpoint(
    value: &fencing::EndpointCheckpointBindingV1,
    visit: &mut impl FnMut(&String) -> Result<(), FormalError>,
) -> Result<(), FormalError> {
    visit(&value.commitment)?;
    visit(&value.head.decision_hash)
}
fn visit_body(
    value: &HotBody,
    visit: &mut impl FnMut(&String) -> Result<(), FormalError>,
) -> Result<(), FormalError> {
    for s in [
        &value.instance_digest,
        &value.authority_id,
        &value.activation_ref,
        &value.head.record_id,
    ] {
        visit(s)?;
    }
    for s in [
        &value.head_node,
        &value.imported_head_node,
        &value.import_digest,
        &value.index,
        &value.pending,
        &value.data_closure,
    ] {
        optional(s.as_ref(), visit)?;
    }
    if let Some(head) = &value.decision_head {
        visit(&head.record_id)?;
    }
    if let Some(parent) = &value.base_parent {
        visit(&parent.finalizing_authority_id)?;
        let record = &parent.record;
        for s in [
            &record.parent_record_id,
            &record.original_authority_id,
            &record.activation_ref,
            &record.request_digest,
            &record.payload_digest,
            &record.runtime_manifest_digest,
            &record.governing_manifest_digest,
            &record.execution_policy_digest,
            &record.before_state_root,
            &record.after_state_root,
            &record.action_root,
            &record.event_root,
            &record.result_root,
            &record.projection_root,
            &record.world.world_id,
            &record.world.chain_id,
            &record.world.genesis_digest,
        ] {
            visit(s)?;
        }
        optional(record.durable_set_transition_digest.as_ref(), visit)?;
        optional(record.frozen_reservation_id.as_ref(), visit)?;
        if let Some(handoff) = &record.handoff {
            visit(&handoff.next_profile)?;
            visit(&handoff.next_authority_digest)?;
        }
        for value in record.artifact_roots.values() {
            visit(value)?;
        }
    }
    if let Some(seal) = &value.seal {
        visit_instance(&seal.instance, visit)?;
        visit(&seal.recovery_operation)?;
        visit(&seal.highest_complete.decision_hash)?;
        visit_checkpoint(&seal.checkpoint_before, visit)?;
        if let Some(request) = &seal.tail.request {
            for s in [
                &request.verified_subject,
                &request.operation_domain,
                &request.nonce_scope,
                &request.request_id,
            ] {
                visit(s)?;
            }
        }
        for s in [
            &seal.tail.frozen_input_digest,
            &seal.tail.frozen_certificate_digest,
            &seal.tail.proposal_digest,
            &seal.tail.replica_prepare_digest,
            &seal.tail.primary_receipt_digest,
            &seal.tail.replica_receipt_digest,
        ] {
            optional(s.as_ref(), visit)?;
        }
    }
    if let Some(binding) = &value.durable_set {
        visit(&binding.transition_digest)?;
        visit(&binding.independent_inventory_digest)?;
        optional(binding.activation_digest.as_ref(), visit)?;
    }
    Ok(())
}
fn role_map_nodes(entries: usize) -> Result<usize, FormalError> {
    // Locked Rust 1.96 Clone::clone_subtree preserves shape without insert or
    // split. Empty clone takes is_empty->new_in even if source has an empty leaf.
    // B=6: nonroot min5, capacity11. Height2 requires >=71 keys. At <=14 keys
    // height1 has just one root key and two leaves. Historical 11-key maps may
    // therefore need three nodes, whereas <=10 keys require a single leaf.
    if entries > 14 {
        return Err(invalid_draft("bounded native draft artifact role count"));
    }
    if entries == 0 {
        return Ok(0);
    }
    let a = align_of::<Option<NonNull<()>>>()
        .max(align_of::<u16>())
        .max(align_of::<ArtifactRole>())
        .max(align_of::<String>());
    let leaf = add(
        add(
            add(
                add(
                    size_of::<Option<NonNull<()>>>(),
                    size_of::<MaybeUninit<u16>>(),
                )?,
                size_of::<u16>(),
            )?,
            mul(11, size_of::<MaybeUninit<ArtifactRole>>())?,
        )?,
        add(mul(11, size_of::<MaybeUninit<String>>())?, mul(5, a - 1)?)?,
    )?;
    if entries <= 10 {
        return Ok(leaf);
    }
    let internal = add(
        add(leaf, mul(12, size_of::<NonNull<()>>())?)?,
        mul(2, a - 1)?,
    )?;
    add(mul(2, leaf)?, internal)
}
fn body_layout(
    value: &HotBody,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<(usize, usize), FormalError> {
    let mut payload = 0;
    visit_body(value, &mut |s| {
        debit(ctx, s.len())?;
        payload = add(payload, s.len())?;
        Ok(())
    })?;
    let map = role_map_nodes(
        value
            .base_parent
            .as_ref()
            .map_or(0, |p| p.record.artifact_roots.len()),
    )?;
    Ok((add(add(size_of::<HotBody>(), payload)?, map)?, payload))
}
fn encode_body(
    value: &HotBody,
    max: usize,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<Vec<u8>>, FormalError> {
    super::context_wire::encode_paid(
        value,
        max,
        ctx,
        "protected signature preimage bound",
        "bounded native draft encoding",
    )
}

pub(in crate::controlled_authority) struct PaidNativeCheckpointDraftV1<'a> {
    store: &'a Store,
    identity: WorkIdentityV1,
    body: ReservedValue<HotBody>,
    before: ReservedValue<fencing::EndpointCheckpointBindingV1>,
    preimage: ReservedValue<Vec<u8>>,
    outer: ReservedValue<()>,
}
impl std::fmt::Debug for PaidNativeCheckpointDraftV1<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaidNativeCheckpointDraftV1")
            .field("revision", &self.body.revision)
            .finish_non_exhaustive()
    }
}
impl PaidNativeCheckpointDraftV1<'_> {
    pub(super) fn source_store(&self) -> &Store {
        self.store
    }
    pub(in crate::controlled_authority) fn body(&self) -> &HotBody {
        &self.body
    }
    pub(in crate::controlled_authority) fn preimage(&self) -> &[u8] {
        self.preimage.as_slice()
    }
    pub(in crate::controlled_authority) fn before_checkpoint(
        &self,
    ) -> &fencing::EndpointCheckpointBindingV1 {
        &self.before
    }
    pub(in crate::controlled_authority) fn before_index(&self) -> Option<&str> {
        self.store.body.index.as_deref()
    }
    pub(in crate::controlled_authority) fn identity(&self) -> WorkIdentityV1 {
        self.identity
    }
    pub(in crate::controlled_authority) fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.body.uses_budget(memory)
            && self.before.uses_budget(memory)
            && self.preimage.uses_budget(memory)
            && self.outer.uses_budget(memory)
    }
    pub(in crate::controlled_authority) fn reserved_bytes(&self) -> usize {
        self.body.reserved_bytes()
            + self.before.reserved_bytes()
            + self.preimage.reserved_bytes()
            + self.outer.reserved_bytes()
    }
    pub(in crate::controlled_authority) fn validate_context(
        &self,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        ctx.consume_steps(1)?;
        if self.identity != ctx.work().identity() || !self.uses_budget(ctx.memory()) {
            return Err(invalid_draft("bounded native draft foreign pool or meter"));
        }
        for s in [
            &self.store.checkpoint.commitment,
            &self.store.checkpoint.head.decision_hash,
            &self.before.commitment,
            &self.before.head.decision_hash,
        ] {
            debit(ctx, s.len())?;
        }
        if self.store.checkpoint != *self.before {
            return Err(invalid_draft("bounded native draft checkpoint changed"));
        }
        Ok(())
    }
}
pub(super) fn prepare_native_checkpoint_draft_bounded_v1<'a>(
    store: &'a Store,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<PaidNativeCheckpointDraftV1<'a>, FormalError> {
    ctx.consume_steps(1)?;
    if store.body.seal.is_some() {
        return Err(FormalError::Conflict("formal epoch sealed".into()));
    }
    let revision = store
        .body
        .revision
        .checked_add(1)
        .ok_or_else(|| invalid("formal revision overflow"))?;
    let (h, payload) = body_layout(&store.body, ctx)?;
    let body_fee = ctx
        .memory()
        .reserve(h, MemoryClass::Ordinary)
        .map_err(|_| invalid_draft("bounded native draft body admission"))?;
    debit(ctx, payload)?;
    let mut next = store.body.clone();
    next.revision = revision;
    visit_body(&next, &mut |s| {
        ctx.consume_steps(1)?;
        if s.capacity() > s.len() {
            return Err(invalid_draft("bounded native draft clone capacity"));
        }
        Ok(())
    })?;
    let body = body_fee.retain(next);
    let before_bytes = add(
        add(
            size_of::<fencing::EndpointCheckpointBindingV1>(),
            store.checkpoint.commitment.len(),
        )?,
        store.checkpoint.head.decision_hash.len(),
    )?;
    let before_fee = ctx
        .memory()
        .reserve(before_bytes, MemoryClass::Ordinary)
        .map_err(|_| invalid_draft("bounded native draft checkpoint admission"))?;
    debit(
        ctx,
        add(
            store.checkpoint.commitment.len(),
            store.checkpoint.head.decision_hash.len(),
        )?,
    )?;
    let before = store.checkpoint.clone();
    if before.commitment.capacity() > before.commitment.len()
        || before.head.decision_hash.capacity() > before.head.decision_hash.len()
    {
        return Err(invalid_draft("bounded native draft checkpoint capacity"));
    }
    let before = before_fee.retain(before);
    let typed = add(
        add(
            size_of::<HotBody>(),
            size_of::<fencing::EndpointCheckpointBindingV1>(),
        )?,
        size_of::<Vec<u8>>(),
    )?;
    let outer_bytes = size_of::<PaidNativeCheckpointDraftV1<'_>>()
        .checked_sub(typed)
        .ok_or_else(|| invalid_draft("bounded native draft owner layout"))?;
    let outer = ctx
        .memory()
        .reserve(outer_bytes, MemoryClass::Ordinary)
        .map_err(|_| invalid_draft("bounded native draft owner admission"))?
        .retain(());
    let prefix = add(8, DOMAIN.len())?;
    let max_body = PREIMAGE_LIMIT
        .checked_sub(prefix)
        .ok_or_else(|| invalid_draft("bounded native draft domain layout"))?;
    let wire = encode_body(&body, max_body, ctx)?;
    let total = add(prefix, wire.len())?;
    let frame_fee = ctx
        .memory()
        .reserve(add(total, size_of::<Vec<u8>>())?, MemoryClass::Ordinary)
        .map_err(|_| invalid_draft("bounded native draft preimage admission"))?;
    let mut preimage = Vec::with_capacity(total);
    if preimage.capacity() > total {
        return Err(invalid_draft("bounded native draft preimage capacity"));
    }
    for source in [
        &(DOMAIN.len() as u64).to_be_bytes()[..],
        DOMAIN.as_bytes(),
        wire.as_slice(),
    ] {
        for chunk in source.chunks(8192) {
            debit(ctx, chunk.len())?;
            preimage.extend_from_slice(chunk);
        }
    }
    if preimage.len() != total || total > PREIMAGE_LIMIT {
        return Err(invalid_draft("protected signature preimage bound"));
    }
    let preimage = frame_fee.retain(preimage);
    drop(wire);
    ctx.consume_steps(1)?;
    Ok(PaidNativeCheckpointDraftV1 {
        store,
        identity: ctx.work().identity(),
        body,
        before,
        preimage,
        outer,
    })
}
#[cfg(test)]
#[path = "endpoint_store_bounded_native_draft_tests.rs"]
mod tests;
