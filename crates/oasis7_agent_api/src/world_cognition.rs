//! Public cognition request identities and pure validation. These DTOs carry
//! correlation, never World admission or durable receipt authority.
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::fmt;

/// World-head identity captured with a continuous-agent request.  This is a
/// value object rather than an adapter-owned assertion: admission compares it
/// with the current World binding before allocating any commit IDs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCognitionBaseBindingV1 {
    pub world_id: String,
    pub branch_id: String,
    pub finality_epoch: u64,
    #[serde(default)]
    pub finality_block_hash: Option<String>,
    pub finality_status: String,
    pub base_tick: u64,
    pub base_world_hash: String,
    pub reorg_epoch: u64,
    pub runtime_manifest_hash: String,
}

impl RuntimeCognitionBaseBindingV1 {
    pub fn validate(&self) -> Result<(), CognitionRecoveryError> {
        let bounded = |value: &str| !value.trim().is_empty() && value.len() <= 256;
        if !bounded(&self.world_id)
            || !bounded(&self.branch_id)
            || !bounded(&self.finality_status)
            || !bounded(&self.base_world_hash)
            || !bounded(&self.runtime_manifest_hash)
            || !finality_binding_is_legal(
                &self.finality_status,
                self.finality_block_hash.as_deref(),
            )
            || !canonical_blake3(&self.base_world_hash)
            || !canonical_blake3(&self.runtime_manifest_hash)
        {
            return Err(CognitionRecoveryError::new(
                "runtime_cognition_base_binding_invalid",
            ));
        }
        Ok(())
    }
}

/// The non-authoritative portion of a continuous-agent turn accepted by the
/// Runtime commit seam.  World derives branch, finality, root, manifest,
/// tick, commit and receipt fields; the adapter supplies only this verified
/// request correlation and its already-mapped Runtime action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCognitionCommitRequestV1 {
    pub agent_id: String,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub retry_seq: u64,
    pub transport_attempt: u64,
    pub request_digest: String,
    pub observation_digest: String,
    /// Canonical digest of the verified outer request context. Runtime treats
    /// this as an input identity and never derives authority from its text.
    pub context_digest: String,
    /// Verified capability/authority snapshots. MvccValidator checks these
    /// against the current World when the World is bound.
    pub capability_snapshot_hash: String,
    pub authority_context_hash: String,
    /// The exact Runtime binding observed while constructing the outer
    /// request.  Runtime compares every field against the live World head;
    /// this prevents a valid request from being committed after a tick or
    /// reorg changed its authority context.
    pub captured_base_binding: RuntimeCognitionBaseBindingV1,
}

impl RuntimeCognitionCommitRequestV1 {
    pub fn validate(&self) -> Result<(), CognitionRecoveryError> {
        let bounded = |value: &str| !value.trim().is_empty() && value.len() <= 256;
        if !bounded(&self.agent_id)
            || !bounded(&self.agent_session_id)
            || !bounded(&self.agent_turn_id)
            || !bounded(&self.decision_request_id)
            || self.retry_seq == 0
            || self.transport_attempt == 0
            || !canonical_blake3(&self.request_digest)
            || !canonical_blake3(&self.observation_digest)
            || !canonical_blake3(&self.context_digest)
            || !canonical_blake3(&self.capability_snapshot_hash)
            || !canonical_blake3(&self.authority_context_hash)
        {
            return Err(CognitionRecoveryError::new(
                "runtime_cognition_commit_request_invalid",
            ));
        }
        self.captured_base_binding.validate()?;
        Ok(())
    }
}

/// Complete outer response/artifact identity emitted by the continuous-agent
/// adapter.  It is retained as the response artifact so replay can validate
/// turn/session/request/retry/transport correlation, not merely content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeCognitionResponseArtifactV1 {
    pub schema_version: u16,
    pub context_discriminator: String,
    pub context_version: u16,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub retry_seq: u64,
    pub transport_attempt: u64,
    pub request_digest: String,
    pub response_digest: String,
    pub artifact_digest: String,
}

impl RuntimeCognitionResponseArtifactV1 {
    pub const CONTEXT_DISCRIMINATOR: &'static str = "oasis7.continuous-agent-context";
    pub const CONTEXT_VERSION: u16 = 1;

    pub fn recompute_artifact_digest(&self) -> String {
        self.recompute_artifact_digest_with_work_v1(
            &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1,
        )
        .expect("cognition payload must be canonically encodable")
        .as_str()
        .to_owned()
    }

    pub fn refresh_artifact_digest(&mut self) {
        self.artifact_digest = self.recompute_artifact_digest();
    }

    pub fn validate(&self) -> Result<(), CognitionRecoveryError> {
        self.validate_with_work_v1(&oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1)
            .expect("cognition payload must be canonically encodable")
    }

    pub fn validate_for_request(
        &self,
        request: &RuntimeCognitionCommitRequestV1,
    ) -> Result<(), CognitionRecoveryError> {
        self.validate_for_request_with_work_v1(
            request,
            &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1,
        )
        .expect("cognition payload must be canonically encodable")
    }
}

fn canonical_blake3(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

pub fn finality_binding_is_legal(status: &str, block_hash: Option<&str>) -> bool {
    matches!(status, "pending" | "verified" | "reorged" | "suspended")
        && match block_hash {
            // A finality hash is optional until the upstream binding is
            // verified, but any supplied hash remains a typed binding.
            Some(hash) => canonical_blake3(hash),
            None => status != "verified",
        }
}

/// The original digest preimage, shared by legacy and paid streaming producers.
pub fn cognition_digest_input_v1<'a, T: Serialize + ?Sized>(
    domain: &'a str,
    payload: &'a T,
) -> (&'a str, &'a T) {
    (domain, payload)
}
#[path = "world_cognition_digest_paid.rs"]
mod paid_digest;
pub use paid_digest::{
    PaidCognitionDigestDataV1, canonical_blake3_data_with_work_v1, cognition_digest_with_work_v1,
};

/// Canonical digest helper shared by World admission, restore and replay.
pub fn cognition_digest_v1<T: Serialize>(domain: &str, payload: &T) -> String {
    cognition_digest_with_work_v1(
        domain,
        payload,
        &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1,
    )
    .expect("cognition payload must be canonically encodable")
    .as_str()
    .to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CognitionRecoveryError {
    code: &'static str,
}

impl CognitionRecoveryError {
    pub fn new(code: &'static str) -> Self {
        Self { code }
    }

    pub fn code(&self) -> &str {
        self.code
    }
}

impl fmt::Display for CognitionRecoveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for CognitionRecoveryError {}

#[path = "world_cognition_response_work.rs"]
mod response_work;
