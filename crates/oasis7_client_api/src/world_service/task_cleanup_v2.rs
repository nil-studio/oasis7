//! New fixed catalog contracts, not aliases of V1 Submit/Lookup domains.
//! No Native gateway producer, paid admission or HTTP client is supplied here.
use super::{CommitRef, SignedReadRequest, WorldIdentity};
use oasis7_agent_api::game::{DecimalU64, GameTaskClosedV2, GameTaskRevokeAndCloseV2};
use serde::{Deserialize, Serialize};
pub const TASK_CLEANUP_SUBMIT_V2: &str = "/v1/world/task-cleanup-submit-v2";
pub const TASK_CLEANUP_LOOKUP_V2: &str = "/v1/world/task-cleanup-lookup-v2";
fn hex(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskCleanupSubmitV2 {
    pub api_revision: u32,
    pub expected_world: WorldIdentity,
    pub original: SignedReadRequest<GameTaskRevokeAndCloseV2>,
}
impl TaskCleanupSubmitV2 {
    pub fn validate(&self) -> Result<(), String> {
        self.expected_world.validate().map_err(|e| e.to_string())?;
        self.original.request.validate()?;
        let f = &self
            .original
            .request
            .permitted_branches
            .exact_initial_active;
        if self.api_revision != 2
            || self.expected_world.world_id != f.world_id
            || self.expected_world.genesis_digest != f.genesis_digest
            || self.original.subject_public_key != f.owner_public_key
        {
            return Err("cleanup v2 world/Owner mismatch".into());
        }
        Ok(())
    }
    /// Signature check only; Runtime still checks the actual canonical Owner.
    pub fn verify_owner_signature(&self) -> Result<(), String> {
        self.validate()?;
        super::private_goal::verify_owner_signature_strict(
            oasis7_agent_api::game::GAME_TASK_CLEANUP_DOMAIN_V2,
            &self.original.request,
            &self.original.subject_public_key,
            &self.original.signature_hex,
        )
    }
    pub fn original_digest(&self) -> Result<String, String> {
        oasis7_wasm_abi::canonical_hash(&self.original).map_err(|e| e.to_string())
    }
}
/// The nonce is a fresh observation correlation, not a second Owner signature
/// or authority grant. A future Native caller must authenticate/pay every HTTP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskCleanupLookupV2 {
    pub api_revision: u32,
    pub submitted: TaskCleanupSubmitV2,
    pub observation_nonce: String,
}
impl TaskCleanupLookupV2 {
    pub fn validate(&self) -> Result<(), String> {
        self.submitted.validate()?;
        if self.api_revision != 2 || !hex(&self.observation_nonce) {
            return Err("invalid cleanup v2 observation nonce".into());
        }
        Ok(())
    }
}
fn required_predecessor<'de, D>(d: D) -> Result<Option<Vec<u8>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<Vec<u8>>::deserialize(d)
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskCleanupFactV2 {
    pub closed: GameTaskClosedV2,
    pub committed_height: DecimalU64,
    pub committed_at_unix_ms: DecimalU64,
    pub closed_at_commit: CommitRef,
    #[serde(deserialize_with = "required_predecessor")]
    pub stop_predecessor_canonical_cbor: Option<Vec<u8>>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskCleanupCurrentV2 {
    pub current_owner_public_key: String,
    pub current_owner_binding: String,
    pub current_owner_generation: DecimalU64,
    pub current_revoked_signer_public_key: String,
    pub current_revoked_generation: DecimalU64,
    pub current_nonce_high_water: DecimalU64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskCleanupDriftV2 {
    OwnerOrLineage,
    SignerOrHighWater,
    DeliveryCommit,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "current",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum TaskCleanupDeliveryV2 {
    ExactClosed(TaskCleanupCurrentV2),
    Drift(TaskCleanupDriftV2),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "outcome",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum TaskCleanupOutcomeV2 {
    Unknown,
    HistoryUnavailable,
    UnresolvedCurrentDrift(TaskCleanupDriftV2),
    KnownClosed {
        fact: Box<TaskCleanupFactV2>,
        delivery: TaskCleanupDeliveryV2,
    },
}
/// A nonce-bound service assertion of fact/current, never an executable grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskCleanupObservationV2 {
    pub api_revision: u32,
    pub world: WorldIdentity,
    pub original_digest: String,
    pub observation_nonce: String,
    pub delivery_commit: CommitRef,
    pub outcome: TaskCleanupOutcomeV2,
}
impl TaskCleanupObservationV2 {
    pub fn validate(&self, request: &TaskCleanupLookupV2) -> Result<(), String> {
        request.validate()?;
        self.delivery_commit.validate().map_err(|e| e.to_string())?;
        if self.api_revision != 2
            || self.world != request.submitted.expected_world
            || self.delivery_commit.world != self.world
            || self.original_digest != request.submitted.original_digest()?
            || self.observation_nonce != request.observation_nonce
        {
            return Err("cleanup observation v2 correlation mismatch".into());
        }
        if let TaskCleanupOutcomeV2::KnownClosed { fact, delivery } = &self.outcome {
            let p = &request.submitted.original.request;
            let f = &p.permitted_branches.exact_initial_active;
            fact.closed_at_commit
                .validate()
                .map_err(|e| e.to_string())?;
            fact.closed.actual_before_fence.validate()?;
            if fact.closed_at_commit.world != self.world
                || fact.closed_at_commit.binding.branch_id != f.branch_id
                || fact.closed_at_commit.binding.reorg_generation != f.reorg_epoch.0
                || fact.closed.schema_revision != 2
                || fact.committed_height.0 == 0
                || fact.committed_at_unix_ms.0 == 0
                || fact.committed_at_unix_ms.0 > p.cleanup_until_ms.0
                || fact.closed.initial_fence != *f
                || fact.closed.revoked_signer_public_key != p.signer_public_key
                || fact.closed.revoked_generation != p.revoke_generation
                || fact.closed.permanent_nonce_high_water != p.revoke_nonce
                || fact.closed.close_original_digest != self.original_digest
                || fact.closed.close_operation_id != p.client_operation_id
                || fact.closed.termination_plan_digest != p.termination_plan_digest
            {
                return Err("cleanup historical fact v2 mismatch".into());
            }
            validate_predecessor(fact, request)?;
            if let TaskCleanupDeliveryV2::ExactClosed(current) = delivery
                && (current.current_owner_public_key != f.owner_public_key
                    || current.current_owner_binding != f.owner_binding
                    || current.current_owner_generation != f.owner_generation
                    || current.current_revoked_signer_public_key != p.signer_public_key
                    || current.current_revoked_generation != p.revoke_generation
                    || current.current_nonce_high_water != p.revoke_nonce
                    || self.delivery_commit.binding.branch_id != f.branch_id
                    || self.delivery_commit.binding.reorg_generation != f.reorg_epoch.0)
            {
                return Err("cleanup observation v2 current drift".into());
            }
        }
        Ok(())
    }
}

/// Verify only against the deployment-pinned Node key, never a key in the packet.
/// A valid fact is still not a paid grant or permission to dispatch a tool.
pub fn verify_cleanup_observation_v2(
    request: &TaskCleanupLookupV2,
    response: &super::SignedServiceResponse<TaskCleanupObservationV2>,
    configured_node_key: &str,
) -> Result<(), String> {
    request.submitted.verify_owner_signature()?;
    if configured_node_key == request.submitted.original.subject_public_key {
        return Err("cleanup Node and Owner keys must be distinct".into());
    }
    super::verify_service_response(
        TASK_CLEANUP_LOOKUP_V2,
        &super::request_digest(TASK_CLEANUP_LOOKUP_V2, request)?,
        response,
        configured_node_key,
    )?;
    let key: [u8; 32] = hex::decode(configured_node_key)
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| "invalid configured Node key length")?;
    let signature: [u8; 64] = hex::decode(&response.signature_hex)
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| "invalid Node signature length")?;
    ed25519_dalek::VerifyingKey::from_bytes(&key)
        .map_err(|e| e.to_string())?
        .verify_strict(
            &super::signing_bytes(
                TASK_CLEANUP_LOOKUP_V2,
                &(&response.request_digest, &response.payload),
            )?,
            &ed25519_dalek::Signature::from_bytes(&signature),
        )
        .map_err(|e| e.to_string())?;
    response.payload.validate(request)
}
#[cfg(test)]
#[path = "task_cleanup_v2_tests.rs"]
mod tests;

fn validate_predecessor(
    current: &TaskCleanupFactV2,
    request: &TaskCleanupLookupV2,
) -> Result<(), String> {
    use oasis7_agent_api::game::*;
    let p = &request.submitted.original.request;
    match &current.closed.selected_predicate {
        GameTaskClosePredicateV2::InitialActive => {
            if current.closed.actual_before_fence != p.permitted_branches.exact_initial_active
                || current.stop_predecessor_canonical_cbor.is_some()
            {
                return Err("unexpected cleanup initial predecessor".into());
            }
        }
        GameTaskClosePredicateV2::OwnStopSuspended(reference) => {
            let intent = p
                .permitted_branches
                .own_stop_intent
                .as_ref()
                .ok_or("OwnStop not approved")?;
            let bytes = current
                .stop_predecessor_canonical_cbor
                .as_ref()
                .ok_or("full guarded Stop predecessor missing")?;
            if bytes.len() > 64 * 1024 {
                return Err("Stop predecessor exceeds bound".into());
            }
            let record: GameTaskStopRecordV2 = serde_cbor::from_slice(bytes)
                .map_err(|_| "invalid full guarded Stop predecessor")?;
            record.validate()?;
            if serde_cbor::to_vec(&record).map_err(|e| e.to_string())? != *bytes
                || record.committed_height > current.committed_height.0
                || record.committed_at_unix_ms > current.committed_at_unix_ms.0
            {
                return Err("noncanonical guarded Stop predecessor".into());
            }
            let stop = &record.original;
            super::private_goal::verify_owner_signature_strict(
                GAME_TASK_STOP_DOMAIN_V2,
                &stop.request,
                &stop.subject_public_key,
                &stop.signature_hex,
            )?;
            let intent_digest = oasis7_wasm_abi::canonical_hash(&(
                GAME_TASK_STOP_DOMAIN_V2,
                &stop.subject_public_key,
                &stop.request,
            ))
            .map_err(|e| e.to_string())?;
            let original_digest =
                oasis7_wasm_abi::canonical_hash(stop).map_err(|e| e.to_string())?;
            if intent.profile != GameStopIntentProfileV2::GuardedTaskStopV2
                || reference.profile != intent.profile
                || stop.request.initial_fence != p.permitted_branches.exact_initial_active
                || stop.subject_public_key != request.submitted.original.subject_public_key
                || stop.request.custody_account_id != p.custody_account_id
                || stop.request.termination_plan_digest != p.termination_plan_digest
                || stop.request.target_executor_public_key != p.signer_public_key
                || stop.request.target_executor_generation != p.signer_generation
                || stop.request.target_executor_nonce != p.signer_nonce
                || stop.request.client_operation_id != intent.operation_id
                || intent_digest != intent.intent_digest
                || reference.operation_id != intent.operation_id
                || reference.original_digest != original_digest
                || reference.canonical_result_digest != record.result_core_digest
            {
                return Err("guarded Stop predecessor intent/domain/Owner mismatch".into());
            }
            let post = GameTaskFenceV1::from_control(&record.after_control);
            if post != reference.post_fence || post != current.closed.actual_before_fence {
                return Err("guarded Stop predecessor post-fence mismatch".into());
            }
        }
    }
    Ok(())
}
