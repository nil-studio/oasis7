//! World-attested public derivation material. Private slots never enter Account.
//! A plain projection is not proof: use the configured-key strict verifiers.
use super::{task_context::*, *};
use crate::world_service_wire::CognitionIntentV1;
use crate::world_service_wire::{SchedulerOperationV1, TaskSchedulerIntentV1};
use oasis7_agent_api::{
    game::TaskContextIdentityV1, world_cognition::RuntimeCognitionBaseBindingV1,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const CONTEXT_PREPARE_ATTESTED_PATH_V2: &str =
    "/v1/world/private-task/context-prepare-attested-v2";
pub const PREPARED_PROJECTION_DOMAIN_V2: &str = "/v1/world/private-task/prepared-projection-v2";
pub const CONTEXT_READ_ATTESTED_PATH_V2: &str = "/v1/world/private-task/context-read-attested-v2";
pub const CAPTURED_PROJECTION_DOMAIN_V2: &str = "/v1/world/private-task/captured-projection-v2";
pub const CONTEXT_MOVE_PROJECTION_PATH_V2: &str =
    "/v1/world/private-task/context-move-projection-v2";
pub const MAX_ATTESTED_CONTEXT_BODY_BYTES_V2: usize = 64 * 1024;
pub const MAX_ATTESTED_CONTEXT_WIRE_BYTES_V2: usize = 256 * 1024;

/// New V2 signed roots are closed without changing the legacy signed wrapper.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldContextAttestationV2<T> {
    pub request_digest: String,
    pub payload: T,
    pub signature_hex: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextProjectionProfileV2 {
    NativeWorldContextV2,
}
/// Commitment to the entire old-domain signed response, not an unsigned subset.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextBodyCommitmentV2 {
    pub canonical_sha256: String,
    pub canonical_bytes: oasis7_agent_api::game::DecimalU64,
}
/// Only the public signature material of an old-domain private response. The
/// attestation binds it; Account cannot verify this signature without the body.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextPrivateResponseProofV2 {
    pub request_digest: String,
    pub signature_hex: String,
}
impl ContextBodyCommitmentV2 {
    pub fn from_signed_body<T: Serialize>(body: &SignedServiceResponse<T>) -> Result<Self, String> {
        let bytes = zeroize::Zeroizing::new(
            oasis7_wasm_abi::encode_canonical_cbor(body).map_err(|e| e.to_string())?,
        );
        if bytes.is_empty() || bytes.len() > MAX_ATTESTED_CONTEXT_BODY_BYTES_V2 {
            return Err("context_too_large".into());
        }
        Ok(Self {
            canonical_sha256: hex::encode(Sha256::digest(bytes.as_slice())),
            canonical_bytes: oasis7_agent_api::game::DecimalU64(bytes.len() as u64),
        })
    }
    pub fn validate(&self) -> Result<(), String> {
        if !nonce(&self.canonical_sha256)
            || self.canonical_bytes.0 == 0
            || self.canonical_bytes.0 > MAX_ATTESTED_CONTEXT_BODY_BYTES_V2 as u64
        {
            return Err("invalid context body commitment".into());
        }
        Ok(())
    }
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskContextPrepareAttestationRequestV2 {
    pub original: SignedReadRequest<TaskContextPrepareV1>,
    pub observation_nonce: String,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskContextPreparedProjectionV2 {
    pub api_version: u8,
    pub profile: ContextProjectionProfileV2,
    pub original: SignedReadRequest<TaskContextPrepareV1>,
    pub observation_nonce: String,
    pub context: TaskContextIdentityV1,
    pub read_commit: CommitRef,
    pub activation_commit: CommitRef,
    pub captured_base_binding: RuntimeCognitionBaseBindingV1,
    pub private_response: ContextPrivateResponseProofV2,
    pub body: ContextBodyCommitmentV2,
}
/// Identity-only delivery type. Never serialize this to an Account RPC.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskContextPreparedDeliveryV2 {
    pub private_body: SignedServiceResponse<PreparedTaskContextV1>,
    pub projection: WorldContextAttestationV2<TaskContextPreparedProjectionV2>,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskContextGetAttestationRequestV2 {
    pub original: SignedReadRequest<TaskContextGetV1>,
    pub observation_nonce: String,
    pub prepared_projection: WorldContextAttestationV2<TaskContextPreparedProjectionV2>,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskContextCapturedProjectionV2 {
    pub api_version: u8,
    pub profile: ContextProjectionProfileV2,
    pub original: SignedReadRequest<TaskContextGetV1>,
    pub observation_nonce: String,
    pub prepared_projection_digest: String,
    pub context: TaskContextIdentityV1,
    pub capture_commit: CommitRef,
    pub read_commit: CommitRef,
    pub read_logical_tick: oasis7_agent_api::game::DecimalU64,
    pub prefix_original_digest: String,
    pub prefix_correlation: RequestCorrelation,
    pub private_response: ContextPrivateResponseProofV2,
    pub body: ContextBodyCommitmentV2,
}
/// Identity-only delivery type; the inner public projection alone goes to Account.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskContextCapturedDeliveryV2 {
    pub private_body: SignedServiceResponse<CapturedTaskContextV1>,
    pub projection: WorldContextAttestationV2<TaskContextCapturedProjectionV2>,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskContextMoveProjectionRequestV2 {
    pub original: SignedReadRequest<TaskContextGetV1>,
    pub observation_nonce: String,
    pub prepared_projection: WorldContextAttestationV2<TaskContextPreparedProjectionV2>,
    pub captured_projection: WorldContextAttestationV2<TaskContextCapturedProjectionV2>,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskContextMoveProjectionV2 {
    pub api_version: u8,
    pub profile: ContextProjectionProfileV2,
    pub observation_nonce: String,
    pub original_get_digest: String,
    pub prepared_projection_digest: String,
    pub captured_projection_digest: String,
    pub context: TaskContextIdentityV1,
    pub capture_commit: CommitRef,
    pub read_commit: CommitRef,
    pub read_logical_tick: oasis7_agent_api::game::DecimalU64,
    pub captured_base_binding: RuntimeCognitionBaseBindingV1,
    pub observation_digest: String,
    pub capability_snapshot_hash: String,
    pub authority_context_hash: String,
    pub body: ContextBodyCommitmentV2,
}
/// Stack-only result of real configured-World verification. No serde/Clone/Debug.
pub struct VerifiedPreparedProjectionV2<'a> {
    projection: &'a TaskContextPreparedProjectionV2,
}
impl VerifiedPreparedProjectionV2<'_> {
    /// Pure reconstruction, never a signer or canonical send permit.
    pub fn prefix_candidate(&self) -> Result<TaskSchedulerIntentV1, String> {
        let p = self.projection;
        let binding = p
            .context
            .fence
            .executor_binding
            .as_ref()
            .ok_or("missing Executor")?;
        Ok(TaskSchedulerIntentV1 {
            agent_id: p.context.fence.agent_id.clone(),
            request_id: p.context.decision_request_id.clone(),
            delegation_generation: binding.delegation_generation.0,
            task_prefix_binding_revision: 1,
            captured_base_binding: p.captured_base_binding.clone(),
            operation: SchedulerOperationV1::TaskProviderPrefix {
                context: p.context.clone(),
            },
        })
    }
    pub fn get_candidate(&self) -> TaskContextGetV1 {
        TaskContextGetV1 {
            schema_revision: 1,
            context: self.projection.context.clone(),
            prepare_operation_id: self.projection.original.request.client_operation_id.clone(),
        }
    }
}
/// Stack-only result of real configured-World verification. No serde/Clone/Debug.
pub struct VerifiedMoveProjectionV2<'a> {
    projection: &'a TaskContextMoveProjectionV2,
}
impl VerifiedMoveProjectionV2<'_> {
    /// A pure fixed-Move SDK reconstruction. Account still supplies only its
    /// already-approved coordinates and must independently check live authority.
    pub fn move_candidate(&self, position_cm: [i64; 3]) -> Result<CognitionIntentV1, String> {
        derive::move_candidate(self.projection, position_cm)
    }
}
pub fn public_projection_digest_v2<T: Serialize>(
    projection: &WorldContextAttestationV2<T>,
) -> Result<String, String> {
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(&(
        "oasis7.world.context-public-projection.v2",
        projection,
    ))
    .map_err(|e| e.to_string())?;
    Ok(hex::encode(Sha256::digest(bytes)))
}
fn nonce(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn verify_attestation<Q: Serialize, T: Serialize>(
    domain: &str,
    request: &Q,
    response: &WorldContextAttestationV2<T>,
    world_key: &str,
) -> Result<(), String> {
    let borrowed = SignedServiceResponse {
        request_digest: response.request_digest.clone(),
        payload: &response.payload,
        signature_hex: response.signature_hex.clone(),
    };
    verify_private_reply(domain, request, &borrowed, world_key)
}
#[path = "task_context_projection_verify.rs"]
mod verify;
pub use verify::*;
#[path = "task_context_projection_derive.rs"]
mod derive;
pub use derive::{
    derive_captured_projection_v2, derive_move_projection_v2, derive_prepared_projection_v2,
};
