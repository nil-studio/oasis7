//! Closed signing data. Only the actual Account/Role Current reader can authorize use.
use crate::{
    DecimalU64,
    approval::ApprovedWorldV1,
    provisioning::{PublicRoleKey, SessionPurposeV1},
    role_key_slot::RoleKeySlotScopeV1,
    time_units::UnixMillis,
};
use oasis7_agent_api::game::GameTaskFenceV1;
use oasis7_client_api::world_service::{
    ReadWorldViewRequest,
    task_context::{TaskContextGetV1, TaskContextPrepareV1},
};
use oasis7_client_api::world_service_wire::{CognitionIntentV1, TaskSchedulerIntentV1};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttestedExecutorGrantProfileV1 {
    OriginalV1MoveExecutor,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "request",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum AttestedExecutorCandidateV1 {
    PrepareContext(TaskContextPrepareV1),
    ReadContext(TaskContextGetV1),
    AccessView(ReadWorldViewRequest),
    Prefix(TaskSchedulerIntentV1),
    MoveCognition(CognitionIntentV1),
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttestedExecutorGrantV1 {
    pub api_version: u8,
    pub profile: AttestedExecutorGrantProfileV1,
    pub session_id: String,
    pub approved_input_digest: String,
    pub action_request_id: String,
    pub action_request_digest: String,
    pub context_operation_id: String,
    pub candidate: AttestedExecutorCandidateV1,
    pub candidate_digest: String,
    pub executor_scope: RoleKeySlotScopeV1,
    pub executor_key: PublicRoleKey,
    pub world: ApprovedWorldV1,
    pub fence: GameTaskFenceV1,
    pub source_operation_id: String,
    pub source_verification_digest: String,
    pub source_observed_at: UnixMillis,
    pub source_expires_at: UnixMillis,
    pub purpose: SessionPurposeV1,
    pub deadline: UnixMillis,
    pub sign_claim_id: String,
    pub registration_operation_id: String,
    pub registration_digest: String,
    pub required_vault_revision: DecimalU64,
    pub paid_port_ids: Vec<String>,
}
// Account builds this from actual original V1 grant + strong Current and paid
// ledger, then runtime retrieves/revalidates it from a fixed private service.
// No public constructor turns this wire into a short-lived signing authority.

impl AttestedExecutorGrantV1 {
    pub fn digest(&self) -> Result<String, String> {
        let bytes = oasis7_wasm_abi::encode_canonical_cbor(&(
            "oasis7.custody.attested-v1-executor-grant.v1",
            self,
        ))
        .map_err(|_| "invalid fixed Executor grant")?;
        Ok(crate::approval::bytes_digest(&bytes))
    }
    /// Structural validation only, never a substitute for fresh Account source,
    /// exact approved candidate, independent Fence or durable Vault once.
    pub fn validate_metadata(&self) -> Result<(), String> {
        fn id(s: &str) -> bool {
            !s.is_empty()
                && s.len() <= 256
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
        }
        fn digest(s: &str) -> bool {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        }
        self.executor_scope
            .validate()
            .map_err(|_| "invalid fixed Executor scope")?;
        self.fence
            .validate()
            .map_err(|_| "invalid fixed Executor fence")?;
        if self.api_version != 1
            || self.required_vault_revision.0 != 2
            || !matches!(
                self.purpose,
                SessionPurposeV1::Execution | SessionPurposeV1::ReconcileRead
            )
            || [
                &self.session_id,
                &self.action_request_id,
                &self.context_operation_id,
                &self.source_operation_id,
                &self.sign_claim_id,
                &self.registration_operation_id,
            ]
            .iter()
            .any(|s| !id(s))
            || [
                &self.approved_input_digest,
                &self.action_request_digest,
                &self.candidate_digest,
                &self.source_verification_digest,
                &self.registration_digest,
            ]
            .iter()
            .any(|s| !digest(s))
            || self.source_observed_at.get() >= self.source_expires_at.get()
            || self.source_expires_at.get() > self.deadline.get()
            || self.paid_port_ids.is_empty()
            || self.paid_port_ids.len()
                > match self.purpose {
                    SessionPurposeV1::Execution => 400,
                    SessionPurposeV1::ReconcileRead => 300,
                    _ => 0,
                }
            || self.paid_port_ids.iter().any(|s| !id(s))
            || self
                .paid_port_ids
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.paid_port_ids.len()
            || self.executor_scope.context.identity.role != crate::http::Role::Executor
            || self
                .fence
                .executor_binding
                .as_ref()
                .is_none_or(|b| b.executor_public_key != self.executor_key.public_key)
            || self.executor_scope.context.identity.key_id != self.executor_key.key_id
            || self.executor_scope.context.identity.key_version != self.executor_key.key_version
        {
            return Err("invalid fixed Executor grant".into());
        }
        let candidate = oasis7_wasm_abi::encode_canonical_cbor(&(
            "oasis7.custody.attested-v1-executor-candidate.v1",
            &self.candidate,
        ))
        .map_err(|_| "invalid fixed Executor candidate")?;
        if crate::approval::bytes_digest(&candidate) != self.candidate_digest {
            return Err("fixed Executor candidate mismatch".into());
        }
        Ok(())
    }
}
