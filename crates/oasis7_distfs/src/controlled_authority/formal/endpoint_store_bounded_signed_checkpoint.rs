//! Paid signed checkpoint data. This owner does not begin or publish anything.
use super::*;
use crate::controlled_authority::fencing::{
    BoundedAcquireErrorV1, EndpointCheckpointBindingV1, PaidNativeCheckpointSignatureV1,
};
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1;
use oasis7_formal_protocol::controlled_authority::formal::crypto::verify_with_preimage;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use std::{cell::RefCell, mem::size_of};
type ResultV<T> = Result<T, BoundedAcquireErrorV1>;
fn add(a: usize, b: usize) -> ResultV<usize> {
    a.checked_add(b)
        .ok_or_else(|| invalid("bounded signed checkpoint layout").into())
}
fn charge(ctx: &mut FormalBoundedContextV1<'_>, n: usize) -> ResultV<()> {
    ctx.charge(
        1,
        u64::try_from(n).map_err(|_| invalid("bounded signed checkpoint layout"))?,
    )?;
    Ok(())
}
fn reserve(
    memory: &MemoryBudget,
    n: usize,
) -> ResultV<oasis7_proto::memory_budget::MemoryReservation> {
    memory
        .reserve(n, MemoryClass::Ordinary)
        .map_err(|_| invalid("bounded signed checkpoint admission").into())
}
fn copy(text: &str, ctx: &mut FormalBoundedContextV1<'_>) -> ResultV<String> {
    charge(ctx, text.len())?;
    let mut value = String::with_capacity(text.len());
    if value.capacity() != text.len() {
        return Err(invalid("bounded signed checkpoint string capacity").into());
    }
    value.push_str(text);
    Ok(value)
}
pub(in crate::controlled_authority) struct PaidSignedNativeCheckpointV1<'a> {
    signed: PaidNativeCheckpointSignatureV1<'a>,
    wire: ReservedValue<Vec<u8>>,
    binding: ReservedValue<EndpointCheckpointBindingV1>,
    outer: ReservedValue<()>,
}
impl std::fmt::Debug for PaidSignedNativeCheckpointV1<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaidSignedNativeCheckpointV1")
            .finish_non_exhaustive()
    }
}
impl PaidSignedNativeCheckpointV1<'_> {
    pub(in crate::controlled_authority) fn bytes(&self) -> &[u8] {
        &self.wire
    }
    pub(in crate::controlled_authority) fn binding(&self) -> &EndpointCheckpointBindingV1 {
        &self.binding
    }
    pub(in crate::controlled_authority) fn source(&self) -> &PaidNativeCheckpointSignatureV1<'_> {
        &self.signed
    }
    pub(in crate::controlled_authority) fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.signed.uses_budget(memory)
            && self.wire.uses_budget(memory)
            && self.binding.uses_budget(memory)
            && self.outer.uses_budget(memory)
    }
    pub(in crate::controlled_authority) fn reserved_bytes(&self) -> usize {
        self.signed.reserved_bytes()
            + self.wire.reserved_bytes()
            + self.binding.reserved_bytes()
            + self.outer.reserved_bytes()
    }
}
pub(in crate::controlled_authority) fn prepare_signed_native_checkpoint_bounded_v1<'a>(
    signed: PaidNativeCheckpointSignatureV1<'a>,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> ResultV<PaidSignedNativeCheckpointV1<'a>> {
    signed.validate_context(ctx)?;
    let body = signed.draft().body();
    let signature = signed.endpoint_signature_hex();
    let wire = super::context_wire::encode_paid(
        &Hot { body, signature },
        64 * 1024,
        ctx,
        "signed native checkpoint byte bound",
        "bounded signed checkpoint encoding",
    )?;
    // hex::decode's bounded canonical key/signature produce 32/64-byte Vecs;
    // both are paid conservatively together, including their headers. Stack
    // key/signature arrays and curve/signature objects remain in this phase.
    let crypto_layout = add(
        add(96, 2 * size_of::<Vec<u8>>())?,
        add(
            96,
            add(
                size_of::<ed25519_dalek::VerifyingKey>(),
                size_of::<ed25519_dalek::Signature>(),
            )?,
        )?,
    )?;
    const DIAGNOSTICS: &[&str] = &[
        "formal digest/key requires canonical lowercase hex32",
        "formal signature requires canonical lowercase hex64",
        "formal public key",
        "formal signature",
        "formal signature verification",
        "invalid canonical hash",
        "bounded signed checkpoint admission",
    ];
    let hash_layout = add(
        add(size_of::<blake3::Hasher>(), size_of::<blake3::Hash>())?,
        add(64, size_of::<usize>())?,
    )?;
    let mut phase_layout = add(
        add(crypto_layout, hash_layout)?,
        add(size_of::<FormalError>(), size_of::<String>())?,
    )?;
    for text in DIAGNOSTICS {
        phase_layout = add(phase_layout, text.len())?;
    }
    let _phase = reserve(ctx.memory(), phase_layout)?;
    let shared = RefCell::new(&mut *ctx);
    Store::verify_generated_hot_with(
        body,
        signature,
        signed.endpoint_public_key(),
        |_, sig, key| {
            verify_with_preimage(
                sig,
                key,
                || Ok::<_, BoundedAcquireErrorV1>(signed.draft().preimage()),
                |n| charge(&mut shared.borrow_mut(), n),
            )
        },
        |durable| durable.validate_with_work(&mut |n| charge(&mut shared.borrow_mut(), n)),
    )?;
    drop(shared);
    let head = body.decision_head.as_ref().unwrap_or(&body.head);
    let binding_layout = add(
        size_of::<EndpointCheckpointBindingV1>(),
        add(64, head.record_id.len())?,
    )?;
    let binding_fee = reserve(ctx.memory(), binding_layout)?;
    let shared = RefCell::new(&mut *ctx);
    let binding = Store::binding_with(
        body,
        &wire,
        |bytes| {
            let mut hasher = blake3::Hasher::new();
            for chunk in bytes.chunks(8192) {
                charge(&mut shared.borrow_mut(), chunk.len())?;
                hasher.update(chunk);
            }
            let hex = hasher.finalize().to_hex();
            copy(hex.as_str(), &mut shared.borrow_mut())
        },
        |text| copy(text, &mut shared.borrow_mut()),
    )?;
    drop(shared);
    let covered = add(
        add(
            size_of::<PaidNativeCheckpointSignatureV1<'a>>(),
            size_of::<Vec<u8>>(),
        )?,
        size_of::<EndpointCheckpointBindingV1>(),
    )?;
    let extra = size_of::<PaidSignedNativeCheckpointV1<'a>>()
        .checked_sub(covered)
        .ok_or_else(|| invalid("bounded signed checkpoint owner layout"))?;
    let outer = reserve(ctx.memory(), extra)?.retain(());
    Ok(PaidSignedNativeCheckpointV1 {
        signed,
        wire,
        binding: binding_fee.retain(binding),
        outer,
    })
}
#[cfg(test)]
#[path = "endpoint_store_bounded_signed_checkpoint_tests.rs"]
mod tests;

// One-way real-owner transfer for provider Begin. No inverse constructor.
pub(in crate::controlled_authority) struct PaidCheckpointBeginSourceV1<'a> {
    signed: PaidNativeCheckpointSignatureV1<'a>,
    wire: ReservedValue<Vec<u8>>,
    binding: ReservedValue<EndpointCheckpointBindingV1>,
    outer: ReservedValue<()>,
}
impl<'a> PaidSignedNativeCheckpointV1<'a> {
    pub(in crate::controlled_authority) fn into_begin_source(
        self,
        ctx: &mut FormalBoundedContextV1<'_>,
    ) -> ResultV<PaidCheckpointBeginSourceV1<'a>> {
        self.signed.validate_context(ctx)?;
        if !self.uses_budget(ctx.memory()) {
            return Err(invalid("bounded Begin source foreign pool").into());
        }
        // Identical owned fields/layout: no clone, detached reservation or refund.
        assert_eq!(
            size_of::<Self>(),
            size_of::<PaidCheckpointBeginSourceV1<'a>>()
        );
        let Self {
            signed,
            wire,
            binding,
            outer,
        } = self;
        Ok(PaidCheckpointBeginSourceV1 {
            signed,
            wire,
            binding,
            outer,
        })
    }
}
impl<'a> PaidCheckpointBeginSourceV1<'a> {
    pub(in crate::controlled_authority) fn consume<T>(
        self,
        run: impl FnOnce(
            PaidNativeCheckpointSignatureV1<'a>,
            ReservedValue<Vec<u8>>,
            ReservedValue<EndpointCheckpointBindingV1>,
            ReservedValue<()>,
        ) -> T,
    ) -> T {
        let Self {
            signed,
            wire,
            binding,
            outer,
        } = self;
        run(signed, wire, binding, outer)
    }
}
