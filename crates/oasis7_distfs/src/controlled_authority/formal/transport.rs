//! Separate authenticated formal-profile wire domain. Loopback listeners require
//! an authenticated encrypted tunnel for cross-host deployment.
use super::{crypto::*, endpoint::*, *};
use crate::controlled_authority::replicated_protocol::{ClosedRecord, EndpointRole};
use ed25519_dalek::{Signer, SigningKey};
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
pub use durable_set::{DurableSetCandidateServerV1, RemoteDurableSetCandidateV1};
pub use server::FormalEndpointServer;
pub(crate) use wire::{Command, Request, RequestBody, Response, ResponseBody, Value};
pub(crate) const REQUEST_DOMAIN: &str = "oasis7.formal.endpoint.rpc.request.v1";
pub(crate) const RESPONSE_DOMAIN: &str = "oasis7.formal.endpoint.rpc.response.v1";
