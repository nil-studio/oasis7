//! Fixed paid reservation verification. No external predicate/verifier Ops.
use super::bounded_context::FormalBoundedContextV1;
use super::crypto::invalid;
use super::frozen::ReservationValidationOps;
use super::*;
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation, ReservedValue};
use std::mem::size_of;

pub(super) struct Frame(ReservedValue<Vec<u8>>);
impl AsRef<[u8]> for Frame {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
pub(super) fn wire<T: serde::Serialize>(
    value: &T,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<Vec<u8>>, FormalError> {
    super::context_wire::encode_paid(
        value,
        64 << 20,
        ctx,
        "formal encoding exceeds byte bound",
        "normal Frozen",
    )
}
pub(super) fn frame<T: serde::Serialize>(
    domain: &str,
    value: &T,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<Frame, FormalError> {
    let payload = wire(value, ctx)?;
    let n = payload
        .len()
        .checked_add(domain.len())
        .and_then(|n| n.checked_add(8))
        .ok_or_else(|| invalid("Frozen frame layout"))?;
    let fee = ctx
        .memory()
        .reserve(
            n.checked_add(size_of::<Vec<u8>>())
                .ok_or_else(|| invalid("Frozen frame layout"))?,
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("Frozen frame admission"))?;
    let mut out = Vec::with_capacity(n);
    if out.capacity() > n {
        return Err(invalid("Frozen frame capacity"));
    }
    for bytes in [
        &(domain.len() as u64).to_be_bytes()[..],
        domain.as_bytes(),
        payload.as_slice(),
    ] {
        for chunk in bytes.chunks(8192) {
            ctx.charge(1, chunk.len() as u64)?;
            out.extend_from_slice(chunk);
        }
    }
    Ok(Frame(fee.retain(out)))
}
pub fn normal_frozen_hash_bounded<T: serde::Serialize>(
    domain: &str,
    value: &T,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<String>, FormalError> {
    let bytes = frame(domain, value, ctx)?;
    let fee = ctx
        .memory()
        .reserve(64 + size_of::<String>(), MemoryClass::Ordinary)
        .map_err(|_| invalid("Frozen digest admission"))?;
    let mut hash = blake3::Hasher::new();
    for chunk in bytes.as_ref().chunks(8192) {
        ctx.charge(1, chunk.len() as u64)?;
        hash.update(chunk);
    }
    ctx.charge(1, 64)?;
    let text = hash.finalize().to_hex().to_string();
    if text.capacity() != 64 {
        return Err(invalid("Frozen digest capacity"));
    }
    Ok(fee.retain(text))
}
struct Ops<'a, 'c> {
    ctx: &'a mut FormalBoundedContextV1<'c>,
    authority: ReservedValue<String>,
    clone_fee: Option<MemoryReservation>,
}
impl ReservationValidationOps for Ops<'_, '_> {
    type Preimage = Frame;
    fn before_validation(
        &mut self,
        reservation: &SignedFormalFrozenV1,
        _: &VerifiedAuthorityV1,
        parent: &VerifiedFormalCommitV1,
    ) -> Result<(), FormalError> {
        // Real wire measurement covers all payload traversal before original
        // fixed validation. No caller can replace those original predicates.
        let encoded = wire(reservation, self.ctx)?;
        let parent_wire = wire(parent.record(), self.ctx)?;
        self.ctx
            .charge(1, (encoded.len() + parent_wire.len()) as u64)?;
        let n = encoded
            .len()
            .checked_mul(4)
            .and_then(|n| n.checked_add((1 << 20) + size_of::<VerifiedFormalReservationV1>()))
            .ok_or_else(|| invalid("Frozen reservation clone layout"))?;
        self.clone_fee = Some(
            self.ctx
                .memory()
                .reserve(n, MemoryClass::Ordinary)
                .map_err(|_| invalid("Frozen reservation clone admission"))?,
        );
        Ok(())
    }
    fn authority_id(&self) -> Option<&str> {
        Some(&self.authority)
    }
    fn preimage(&mut self, body: &FormalFrozenBodyV1) -> Result<Frame, FormalError> {
        frame("oasis7.formal.frozen_reservation.sign.v1", body, self.ctx)
    }
    fn crypto_work(&mut self, n: usize) -> Result<(), FormalError> {
        self.ctx.charge(1, n as u64)
    }
    fn before_clone(&mut self, reservation: &SignedFormalFrozenV1) -> Result<(), FormalError> {
        let encoded = wire(reservation, self.ctx)?;
        self.ctx.charge(1, encoded.len() as u64)
    }
}
pub fn verify_formal_reservation_bounded(
    reservation: &SignedFormalFrozenV1,
    authority: &VerifiedAuthorityV1,
    parent: &VerifiedFormalCommitV1,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<VerifiedFormalReservationV1>, FormalError> {
    ctx.consume_steps(1)?;
    let _phase = ctx
        .memory()
        .reserve(1 << 20, MemoryClass::Ordinary)
        .map_err(|_| invalid("Frozen validation phase admission"))?;
    let authority_wire = wire(authority.definition(), ctx)?;
    ctx.charge(1, authority_wire.len() as u64)?;
    authority.definition().validate()?;
    let id = normal_frozen_hash_bounded(
        super::record::AUTHORITY_ID_DOMAIN_V1,
        authority.definition(),
        ctx,
    )?;
    let mut ops = Ops {
        ctx,
        authority: id,
        clone_fee: None,
    };
    let value =
        super::frozen::verify_formal_reservation_with(reservation, authority, parent, &mut ops)?;
    ops.ctx.consume_steps(1)?;
    let fee = ops
        .clone_fee
        .take()
        .ok_or_else(|| invalid("Frozen reservation admission missing"))?;
    Ok(fee.retain(value))
}
