//! Explicit full-sync transport dependency. No implementation supplied here is
//! an authority or a proof verifier; the ordinary signed-response checks remain
//! in WorldServiceTransport. A shared implementation owns its original durable
//! operation across endpoints and reader clones, and must never reset on errors.
use crate::WorldServiceClientError;
use std::time::Duration;

pub struct EncodedFullSyncExchange<'a> {
    pub endpoint: &'a str,
    pub path: &'a str,
    pub signed_body: &'a [u8],
    pub response_bound: usize,
    pub request_timeout: Duration,
}

/// Exact canonical manifest object retained in the same candidate pin set.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullSyncManifestReference {
    pub content_hash: String,
    pub size_bytes: u64,
}

pub struct FullSyncHttpResponse {
    pub status: u16,
    pub retry_after: Option<String>,
    pub body: Vec<u8>,
}

/// Implementations must persist admission before network I/O, bound buffers,
/// enforce the original operation deadline and settle actual body I/O. Returning
/// an error never authorizes fallback to the ordinary transport. This interface
/// does not make a callback or in-memory counter a durable operation ledger.
pub trait FullSyncExchange: Send + Sync {
    fn original_request_digest(&self) -> Result<String, WorldServiceClientError>;
    /// Persist the complete locally verified cut and original record before
    /// material I/O. This is identity retention, not independent verification.
    fn bind_verified_target(&self, target_json: &[u8]) -> Result<(), WorldServiceClientError>;
    /// Journal exact local CAS publication before changing roots. Completion
    /// means only roots were acknowledged, never World installation/readiness.
    fn begin_publication(
        &self,
        cut: &serde_json::Value,
        roots: &std::collections::BTreeSet<String>,
        manifest: &FullSyncManifestReference,
    ) -> Result<(), WorldServiceClientError>;
    fn complete_publication(
        &self,
        cut: &serde_json::Value,
        roots: &std::collections::BTreeSet<String>,
        manifest: &FullSyncManifestReference,
    ) -> Result<(), WorldServiceClientError>;
    /// Separate retaining-consumer intent; pins alone are not installation.
    fn begin_installation(
        &self,
        cut: &serde_json::Value,
        roots: &std::collections::BTreeSet<String>,
        manifest: &FullSyncManifestReference,
    ) -> Result<(), WorldServiceClientError>;
    /// Acknowledge the live retained candidate and atomically consume its permit.
    fn complete_installation(
        &self,
        cut: &serde_json::Value,
        roots: &std::collections::BTreeSet<String>,
        manifest: &FullSyncManifestReference,
    ) -> Result<(), WorldServiceClientError>;
    fn check_active(&self) -> Result<(), WorldServiceClientError>;
    fn finish_attempt(&self) -> Result<(), WorldServiceClientError>;
    fn charge_proof(&self, encoded_proof_bytes: u64) -> Result<(), WorldServiceClientError>;
    fn charge_material(&self, digest: &str, bytes: u64) -> Result<(), WorldServiceClientError>;
    fn exchange(
        &self,
        request: EncodedFullSyncExchange<'_>,
    ) -> Result<FullSyncHttpResponse, WorldServiceClientError>;
}
