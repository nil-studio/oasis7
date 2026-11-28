//! Paid Request authentication buffers; original predicates confer authority.
use super::*;
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::FormalBoundedContextV1;
use oasis7_formal_protocol::controlled_authority::formal::context_wire::encode_paid;
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation, ReservedValue};
use std::{cell::RefCell, mem::size_of};

const LIMIT: usize = 64 << 20;
const DIGEST_DOMAIN: &str = "oasis7.formal.endpoint.rpc.request_digest.v1";
struct Preimage(ReservedValue<Vec<u8>>);
impl AsRef<[u8]> for Preimage {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}
fn encoded<T: Serialize>(
    value: &T,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<Vec<u8>>, FormalError> {
    encode_paid(
        value,
        LIMIT,
        ctx,
        "formal encoding exceeds byte bound",
        "formal RPC request encoding",
    )
}
fn frame<T: Serialize>(
    domain: &str,
    value: &T,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<Preimage, FormalError> {
    let payload = encoded(value, ctx)?;
    let length = payload
        .len()
        .checked_add(domain.len())
        .and_then(|n| n.checked_add(8))
        .ok_or_else(|| invalid("formal RPC request preimage layout"))?;
    let capacity = length
        .checked_add(4096)
        .ok_or_else(|| invalid("formal RPC request preimage layout"))?;
    let allocation = capacity
        .checked_add(size_of::<Vec<u8>>())
        .ok_or_else(|| invalid("formal RPC request preimage layout"))?;
    ctx.consume_steps(1)?;
    let fee = ctx
        .memory()
        .reserve(allocation, MemoryClass::Ordinary)
        .map_err(|_| invalid("formal RPC request preimage admission"))?;
    let mut bytes = Vec::with_capacity(length);
    if bytes.capacity() > capacity {
        return Err(invalid("formal RPC request preimage capacity"));
    }
    for part in [
        &(domain.len() as u64).to_be_bytes()[..],
        domain.as_bytes(),
        payload.as_slice(),
    ] {
        for chunk in part.chunks(8192) {
            ctx.charge(1, chunk.len() as u64)?;
            bytes.extend_from_slice(chunk);
        }
    }
    Ok(Preimage(fee.retain(bytes)))
}
pub(super) fn verify_request(
    body: &RequestBody,
    signature: &str,
    key: &str,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<(), FormalError> {
    ctx.consume_steps(1)?;
    // Original hex decoding and verifier phase, separate from payload/frame.
    let _phase = ctx
        .memory()
        .reserve(
            4096 + 96
                + 2 * size_of::<Vec<u8>>()
                + size_of::<ed25519_dalek::VerifyingKey>()
                + size_of::<ed25519_dalek::Signature>(),
            MemoryClass::Ordinary,
        )
        .map_err(|_| invalid("formal RPC request crypto admission"))?;
    {
        let shared = RefCell::new(&mut *ctx);
        verify_with_preimage(
            signature,
            key,
            || frame(REQUEST_DOMAIN, body, &mut shared.borrow_mut()),
            |n| shared.borrow_mut().charge(1, n as u64),
        )?;
    }
    ctx.consume_steps(1)
}
pub(super) fn request_digest(
    body: &RequestBody,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<(String, MemoryReservation), FormalError> {
    let payload = encoded(body, ctx)?;
    ctx.consume_steps(1)?;
    let fee = ctx
        .memory()
        .reserve(64 + size_of::<String>(), MemoryClass::Ordinary)
        .map_err(|_| invalid("formal RPC request digest admission"))?;
    let mut hash = blake3::Hasher::new();
    // Same original domain preimage, streamed without a second full Vec.
    for part in [
        &(DIGEST_DOMAIN.len() as u64).to_be_bytes()[..],
        DIGEST_DOMAIN.as_bytes(),
        payload.as_slice(),
    ] {
        for chunk in part.chunks(8192) {
            ctx.charge(1, chunk.len() as u64)?;
            hash.update(chunk);
        }
    }
    ctx.charge(1, 64)?;
    let digest = hash.finalize().to_hex().to_string();
    if digest.capacity() != 64 {
        return Err(invalid("formal RPC request digest capacity"));
    }
    // Private transfer: serve_stream keeps this fee while the String moves
    // into ResponseBody and remains live through physical response output.
    Ok((digest, fee))
}

#[cfg(test)]
#[path = "server_request_auth_tests.rs"]
mod tests;
