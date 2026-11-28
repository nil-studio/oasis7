//! Separate authenticated formal-profile wire domain. Loopback listeners require
//! an authenticated encrypted tunnel for cross-host deployment.
use super::{crypto::*, endpoint::*, *};
use crate::controlled_authority::replicated_protocol::{ClosedRecord, EndpointRole};
use ed25519_dalek::{Signer, SigningKey};
use oasis7_formal_protocol::controlled_authority::formal::{
    bounded_context::FormalBoundedContextV1, context_wire::encode_paid,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use serde::{Deserialize, Serialize};
mod archive_delegation;
mod client;
mod durable_set;
pub use archive_delegation::{
    ArchiveReadGrantBodyV1, SignedArchiveReadGrantV1, archive_read_grant_signing_bytes,
};
mod server;
mod wire;
pub(crate) use client::VerifiedNativeRpcContextV1;
pub use client::{PaidRemoteFormalQueryV1, RemoteFormalEndpoint, VerifiedEndpointObservationV1};
pub use durable_set::{DurableSetCandidateServerV1, RemoteDurableSetCandidateV1};
pub use server::{FormalEndpointServer, FormalRpcFrameAdmissionLimitsV1};
pub(crate) use wire::{Command, Request, RequestBody, Response, ResponseBody, Value};
pub(crate) const REQUEST_DOMAIN: &str = "oasis7.formal.endpoint.rpc.request.v1";
pub(crate) const RESPONSE_DOMAIN: &str = "oasis7.formal.endpoint.rpc.response.v1";
pub(crate) const RESPONSE_DIGEST_DOMAIN: &str = "oasis7.formal.endpoint.rpc.response_body.v1";

// Shared original response digest for the server signer and caller verifier.
pub(crate) fn paid_response_digest(
    body: &ResponseBody,
    ctx: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<String>, FormalError> {
    let payload = encode_paid(
        body,
        64 << 20,
        ctx,
        "formal encoding exceeds byte bound",
        "formal RPC response body encoding",
    )?;
    ctx.consume_steps(1)?;
    // Admit the digest while its input payload is still live in the same pool.
    let fee = ctx
        .memory()
        .reserve(64 + std::mem::size_of::<String>(), MemoryClass::Ordinary)
        .map_err(|_| invalid("formal RPC response digest admission"))?;
    let mut hash = blake3::Hasher::new();
    for part in [
        &(RESPONSE_DIGEST_DOMAIN.len() as u64).to_be_bytes()[..],
        RESPONSE_DIGEST_DOMAIN.as_bytes(),
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
        return Err(invalid("formal RPC response digest capacity"));
    }
    Ok(fee.retain(digest))
}
