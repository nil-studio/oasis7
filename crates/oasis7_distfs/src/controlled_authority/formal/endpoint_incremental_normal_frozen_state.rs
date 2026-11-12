//! Paid original RequestState construction; no shadow RequestState codec.
use super::*;
use crate::controlled_authority::formal::endpoint::store::bounded_request::{
    PaidRequestStateV1, decode_request_bytes_bounded_v1,
};
pub(super) fn state(
    input: &SignedFormalFrozenV1,
    material: &ClosedRecord,
    receipt: Option<&FormalFrozenReceiptV1>,
    certificate: Option<&FormalFrozenCertificateV1>,
    memory: &MemoryBudget,
    work: &FormalArchiveReadWorkV1,
    stop: &mut dyn FnMut() -> bool,
) -> Result<PaidRequestStateV1, FormalError> {
    let mut ctx = work.bounded_context(memory, stop);
    let measured = store::context_wire::encode_paid(
        &(input, material, receipt, certificate),
        64 << 20,
        &mut ctx,
        "formal object byte limit",
        "normal Frozen data",
    )?;
    let n = measured
        .len()
        .checked_mul(8)
        .and_then(|n| n.checked_add((1 << 20) + size_of::<FormalRequestStateV1>()))
        .ok_or_else(|| invalid("normal Frozen state clone layout"))?;
    let fee = memory
        .reserve(n, MemoryClass::Ordinary)
        .map_err(|_| invalid("normal Frozen state admission"))?;
    ctx.charge(1, measured.len() as u64)?;
    let value = fee.retain(FormalRequestStateV1::Frozen {
        input: input.clone(),
        material: material.clone(),
        receipt: receipt.cloned(),
        certificate: certificate.cloned(),
    });
    drop(measured);
    let wire = store::context_wire::encode_paid(
        &*value,
        64 << 20,
        &mut ctx,
        "formal object byte limit",
        "normal Frozen RequestState",
    )?;
    drop(ctx);
    decode_request_bytes_bounded_v1(&wire, memory, work, stop)
}
pub(super) fn input(
    state: &PaidRequestStateV1,
) -> Result<(&SignedFormalFrozenV1, &ClosedRecord), FormalError> {
    match state.state() {
        FormalRequestStateV1::Frozen {
            input, material, ..
        } => Ok((input, material)),
        _ => Err(invalid("normal Frozen state variant")),
    }
}
