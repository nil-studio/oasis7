//! Guarded Stop V2 component. These implementation catalog constants do not
//! mount a route or prove Native/Account prepaid authentication.
use super::{CommitRef, SignedServiceResponse, WorldIdentity};
use oasis7_agent_api::game::{
    GAME_TASK_STOP_DOMAIN_V2, GameTaskFenceV1, GameTaskStopRecordV2, SignedGameTaskStopV2,
};
use serde::{Deserialize, Serialize};
fn hex_nonce(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub const TASK_STOP_SUBMIT_V2: &str = "/v1/world/task-stop-submit-v2";
pub const TASK_STOP_LOOKUP_V2: &str = "/v1/world/task-stop-lookup-v2";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskStopSubmitV2 {
    pub api_revision: u32,
    pub expected_world: WorldIdentity,
    pub original: SignedGameTaskStopV2,
    pub observation_nonce: String,
}
impl TaskStopSubmitV2 {
    pub fn validate(&self) -> Result<(), String> {
        self.expected_world.validate().map_err(|e| e.to_string())?;
        self.original.validate_shape()?;
        let f = &self.original.request.initial_fence;
        if !hex_nonce(&self.observation_nonce)
            || self.api_revision != 2
            || self.expected_world.world_id != f.world_id
            || self.expected_world.genesis_digest != f.genesis_digest
        {
            return Err("guarded Stop World mismatch".into());
        }
        Ok(())
    }
    pub fn verify_owner_signature(&self) -> Result<(), String> {
        self.validate()?;
        super::private_goal::verify_owner_signature_strict(
            GAME_TASK_STOP_DOMAIN_V2,
            &self.original.request,
            &self.original.subject_public_key,
            &self.original.signature_hex,
        )
    }
    pub fn original_digest(&self) -> Result<String, String> {
        oasis7_wasm_abi::canonical_hash(&self.original).map_err(|e| e.to_string())
    }
}
/// The observation nonce correlates a paid authenticated read, never grants it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskStopLookupV2 {
    pub api_revision: u32,
    pub submitted: TaskStopSubmitV2,
    pub observation_nonce: String,
}
impl TaskStopLookupV2 {
    pub fn validate(&self) -> Result<(), String> {
        self.submitted.validate()?;
        if self.api_revision != 2
            || self.observation_nonce.len() != 64
            || !self
                .observation_nonce
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("invalid guarded Stop observation correlation".into());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskStopFactV2 {
    pub record: GameTaskStopRecordV2,
    pub stopped_at_commit: CommitRef,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "current",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum TaskStopDeliveryV2 {
    ExactPostFence(Box<GameTaskFenceV1>),
    Drift,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "outcome",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum TaskStopOutcomeV2 {
    Unknown,
    HistoryUnavailable,
    KnownStopped {
        fact: Box<TaskStopFactV2>,
        delivery: TaskStopDeliveryV2,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskStopObservationV2 {
    pub api_revision: u32,
    pub world: WorldIdentity,
    pub original_digest: String,
    pub observation_nonce: String,
    pub delivery_commit: CommitRef,
    pub outcome: TaskStopOutcomeV2,
}
impl TaskStopObservationV2 {
    pub fn validate(&self, request: &TaskStopLookupV2) -> Result<(), String> {
        request.validate()?;
        self.delivery_commit.validate().map_err(|e| e.to_string())?;
        let f = &request.submitted.original.request.initial_fence;
        if self.api_revision != 2
            || self.world != request.submitted.expected_world
            || self.delivery_commit.world != self.world
            || self.original_digest != request.submitted.original_digest()?
            || self.observation_nonce != request.observation_nonce
        {
            return Err("guarded Stop observation scope mismatch".into());
        }
        if let TaskStopOutcomeV2::KnownStopped { fact, delivery } = &self.outcome {
            fact.record.validate()?;
            fact.stopped_at_commit
                .validate()
                .map_err(|e| e.to_string())?;
            if fact.record.committed_height != fact.stopped_at_commit.position
                || fact.record.original != request.submitted.original
                || fact.stopped_at_commit.world != self.world
                || fact.stopped_at_commit.binding.branch_id != f.branch_id
                || fact.stopped_at_commit.binding.reorg_generation != f.reorg_epoch.0
                || !self
                    .delivery_commit
                    .satisfies_minimum(&fact.stopped_at_commit)
                    .map_err(|e| e.to_string())?
            {
                return Err("guarded Stop historical witness mismatch".into());
            }
            super::private_goal::verify_owner_signature_strict(
                GAME_TASK_STOP_DOMAIN_V2,
                &fact.record.original.request,
                &fact.record.original.subject_public_key,
                &fact.record.original.signature_hex,
            )?;
            if let TaskStopDeliveryV2::ExactPostFence(current) = delivery {
                current.validate()?;
                if **current != GameTaskFenceV1::from_control(&fact.record.after_control) {
                    return Err("guarded Stop delivery current drift".into());
                }
            }
        }
        Ok(())
    }
}
/// Fixed deployment Node trust only; a strong historical fact never grants a send.
pub fn verify_stop_observation_v2(
    request: &TaskStopLookupV2,
    response: &SignedServiceResponse<TaskStopObservationV2>,
    configured_node_key: &str,
) -> Result<(), String> {
    request.submitted.verify_owner_signature()?;
    verify_node(
        TASK_STOP_LOOKUP_V2,
        &super::request_digest(TASK_STOP_LOOKUP_V2, request)?,
        response,
        configured_node_key,
        &request.submitted.original.subject_public_key,
    )?;
    response.payload.validate(request)
}
/// Submit response has its own request domain and exact persisted observation nonce.
pub fn verify_stop_submit_observation_v2(
    request: &TaskStopSubmitV2,
    response: &SignedServiceResponse<TaskStopObservationV2>,
    configured_node_key: &str,
) -> Result<(), String> {
    request.verify_owner_signature()?;
    verify_node(
        TASK_STOP_SUBMIT_V2,
        &super::request_digest(TASK_STOP_SUBMIT_V2, request)?,
        response,
        configured_node_key,
        &request.original.subject_public_key,
    )?;
    let lookup = TaskStopLookupV2 {
        api_revision: 2,
        submitted: request.clone(),
        observation_nonce: request.observation_nonce.clone(),
    };
    response.payload.validate(&lookup)
}
fn verify_node(
    domain: &str,
    digest: &str,
    response: &SignedServiceResponse<TaskStopObservationV2>,
    key_hex: &str,
    owner: &str,
) -> Result<(), String> {
    if key_hex == owner {
        return Err("guarded Stop Node/Owner key separation required".into());
    }
    super::verify_service_response(domain, digest, response, key_hex)?;
    let key: [u8; 32] = hex::decode(key_hex)
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| "invalid Node key length")?;
    let sig: [u8; 64] = hex::decode(&response.signature_hex)
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| "invalid Node signature length")?;
    ed25519_dalek::VerifyingKey::from_bytes(&key)
        .map_err(|e| e.to_string())?
        .verify_strict(
            &super::signing_bytes(domain, &(&response.request_digest, &response.payload))?,
            &ed25519_dalek::Signature::from_bytes(&sig),
        )
        .map_err(|e| e.to_string())
}
#[cfg(test)]
#[path = "task_stop_v2_tests.rs"]
mod tests;
