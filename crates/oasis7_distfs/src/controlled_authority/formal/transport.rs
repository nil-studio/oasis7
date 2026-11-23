//! Separate authenticated formal-profile wire domain. Loopback listeners require
//! an authenticated encrypted tunnel for cross-host deployment.
use super::{crypto::*, endpoint::*, *};
use crate::controlled_authority::replicated_protocol::{ClosedRecord, EndpointRole};
use ed25519_dalek::SigningKey;
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
pub use client::{RemoteFormalEndpoint, VerifiedEndpointObservationV1};
pub use durable_set::{
    DurableSetCandidateServerV1, RemoteDurableSetCandidateV1, WriterDurableSetRequestV1,
};
pub use server::FormalEndpointServer;
pub(crate) use wire::{Command, Request, RequestBody, Response, ResponseBody, Value};
pub(crate) const REQUEST_DOMAIN: &str = "oasis7.formal.endpoint.rpc.request.v1";
pub(crate) const RESPONSE_DOMAIN: &str = "oasis7.formal.endpoint.rpc.response.v1";

pub(crate) fn writer_custody_exchange<Q: Serialize, A: serde::de::DeserializeOwned>(
    address: &std::net::SocketAddr,
    request: &Q,
    deadline: std::time::Instant,
) -> Result<A, FormalError> {
    wire::address(*address)?;
    let mut stream = wire::connect_and_write_before_deadline(address, request, deadline)?;
    wire::read_before_deadline(&mut stream, deadline)
}

pub(crate) fn writer_custody_serve_connection(
    mut stream: std::net::TcpStream,
    timeout: std::time::Duration,
    handle: impl FnOnce(
        SignedWriterCustodyRequestV1,
    ) -> Result<SignedWriterCustodyResponseV1, FormalError>,
) -> Result<(), FormalError> {
    wire::configure(&stream, timeout)?;
    let deadline = std::time::Instant::now() + timeout;
    let request = wire::read_before_deadline(&mut stream, deadline)?;
    let response = handle(request)?;
    wire::write_before_deadline(&mut stream, &response, deadline)
}
