//! Single exact Stop fact/control query. No dispatcher, payment or Current grant.
use super::{task_stop_v2::*, *};
use crate::world_service_wire::AgentSignerDelegationChangeV1;
use oasis7_agent_api::game::{DecimalU64, GameTaskFenceV1, GameTaskStatusV1};
use serde::{Deserialize, Serialize};
pub const TASK_STOP_RECORD_CONTROL_V2: &str = "/v1/world/task-stop-record-control-v2";
pub const STOP_RECORD_WITNESS_DOMAIN_V2: &str = "oasis7.world.stop-record-witness.v2";
pub const STOP_RECORD_CONTROL_CURRENT_DOMAIN_V2: &str =
    "oasis7.world.stop-record-control-current.v2";
fn hex64(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
fn id(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
}
/// Correlation only. The Account producer must separately consume its actual
/// TermWide Book/policy/paid once; deserializing this is not that authorization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopRecordSourceV2 {
    pub approved_input_digest: String,
    pub session_id: String,
    pub invocation_id: String,
    pub source_operation_id: String,
    pub source_nonce: String,
    pub world_attempt_id: String,
}
impl StopRecordSourceV2 {
    pub fn validate(&self) -> Result<(), String> {
        if !hex64(&self.approved_input_digest)
            || !hex64(&self.source_nonce)
            || ![
                &self.session_id,
                &self.invocation_id,
                &self.source_operation_id,
                &self.world_attempt_id,
            ]
            .into_iter()
            .all(|v| id(v))
        {
            return Err("invalid Stop known Source correlation".into());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopRecordControlRequestV2 {
    pub api_revision: u8,
    pub original: TaskStopSubmitV2,
    pub source: StopRecordSourceV2,
}
impl StopRecordControlRequestV2 {
    pub fn validate(&self) -> Result<(), String> {
        self.source.validate()?;
        self.original.verify_owner_signature()?;
        self.original
            .original
            .request
            .observation_grant
            .as_ref()
            .ok_or("Owner combined observation grant missing")?
            .validate_for(&self.original.original.request)?;
        if self.api_revision != 2 {
            return Err("invalid Stop known profile".into());
        }
        Ok(())
    }
    pub fn digest(&self) -> Result<String, String> {
        self.validate()?;
        request_digest(TASK_STOP_RECORD_CONTROL_V2, self)
    }
}
/// Current-root witness, never the original Stop block receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopRecordWitnessFactV2 {
    pub record: oasis7_agent_api::game::GameTaskStopRecordV2,
    pub witnessed_in_commit: CommitRef,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "outcome",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum StopRecordFactV2 {
    Unknown,
    HistoryUnavailable,
    Known(Box<StopRecordWitnessFactV2>),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopRecordFactProjectionV2 {
    pub api_revision: u8,
    pub original_digest: String,
    pub source: StopRecordSourceV2,
    pub observed_at_ms: DecimalU64,
    pub fact: StopRecordFactV2,
}
impl StopRecordFactProjectionV2 {
    pub fn validate(&self, q: &StopRecordControlRequestV2) -> Result<(), String> {
        q.validate()?;
        let grant = q
            .original
            .original
            .request
            .observation_grant
            .as_ref()
            .ok_or("Owner combined observation grant missing")?;
        if self.observed_at_ms.0 == 0 || self.observed_at_ms.0 >= grant.historical_read_until_ms() {
            return Err("Stop historical observation expired".into());
        }
        if self.api_revision != 2
            || self.source != q.source
            || self.original_digest != q.original.original_digest()?
        {
            return Err("Stop fact request mismatch".into());
        }
        if let StopRecordFactV2::Known(fact) = &self.fact {
            fact.record.validate()?;
            fact.witnessed_in_commit
                .validate()
                .map_err(|e| e.to_string())?;
            let f = &q.original.original.request.initial_fence;
            if fact.record.committed_at_unix_ms > self.observed_at_ms.0
                || fact.record.original != q.original.original
                || fact.witnessed_in_commit.world != q.original.expected_world
                || fact.witnessed_in_commit.binding.branch_id != f.branch_id
                || fact.witnessed_in_commit.binding.reorg_generation != f.reorg_epoch.0
                || fact.record.after_control.task_status != GameTaskStatusV1::Suspended
            {
                return Err("Stop fact historical scope mismatch".into());
            }
        }
        Ok(())
    }
}
/// No goal/context body or general World projection is present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopExactControlV2 {
    pub tip: CommitRef,
    pub fence: GameTaskFenceV1,
    pub signer_delegation: AgentSignerDelegationChangeV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "current",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum StopControlDeliveryV2 {
    Unavailable,
    Drift,
    Exact(Box<StopExactControlV2>),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopControlProjectionV2 {
    pub api_revision: u8,
    pub original_digest: String,
    pub source: StopRecordSourceV2,
    pub delivered_at_ms: DecimalU64,
    pub current: StopControlDeliveryV2,
}
impl StopControlProjectionV2 {
    pub fn validate(
        &self,
        q: &StopRecordControlRequestV2,
        fact: &StopRecordFactProjectionV2,
    ) -> Result<(), String> {
        fact.validate(q)?;
        if self.delivered_at_ms.0 < fact.observed_at_ms.0 {
            return Err("Stop delivery clock regressed".into());
        }
        if self.api_revision != 2
            || self.source != q.source
            || self.original_digest != q.original.original_digest()?
        {
            return Err("Stop control request mismatch".into());
        }
        if let StopControlDeliveryV2::Exact(current) = &self.current {
            let grant = q
                .original
                .original
                .request
                .observation_grant
                .as_ref()
                .ok_or("Owner combined observation grant missing")?;
            if self.delivered_at_ms.0 >= grant.control_read_until_ms() {
                return Err("Stop control delivery expired".into());
            }
            let StopRecordFactV2::Known(fact) = &fact.fact else {
                return Err("Stop current without canonical fact".into());
            };
            current.tip.validate().map_err(|e| e.to_string())?;
            current.fence.validate()?;
            let p = &q.original.original.request;
            let f = &p.initial_fence;
            let d = &current.signer_delegation;
            if current.fence != GameTaskFenceV1::from_control(&fact.record.after_control)
                || current.tip.world != q.original.expected_world
                || !current
                    .tip
                    .satisfies_minimum(&fact.witnessed_in_commit)
                    .map_err(|e| e.to_string())?
                || current.tip.binding.branch_id != f.branch_id
                || current.tip.binding.reorg_generation != f.reorg_epoch.0
                || d.world_id != f.world_id
                || d.branch_id != f.branch_id
                || d.agent_id != f.agent_id
                || d.owner_binding != f.owner_binding
                || d.agent_identity_generation != f.owner_generation.0
                || d.revoked
                || d.delegate_public_key != p.target_executor_public_key
                || d.generation != p.target_executor_generation.0
                || d.nonce != p.target_executor_nonce.0
            {
                return Err("Stop control current drift".into());
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopRecordControlResponseV2 {
    pub fact: SignedServiceResponse<StopRecordFactProjectionV2>,
    pub current: SignedServiceResponse<StopControlProjectionV2>,
}
fn strict_node<T: Serialize>(
    domain: &str,
    q: &StopRecordControlRequestV2,
    s: &SignedServiceResponse<T>,
    node: &str,
) -> Result<(), String> {
    if node == q.original.original.subject_public_key {
        return Err("Node/Owner key collision".into());
    }
    verify_service_response(domain, &q.digest()?, s, node)?;
    let key: [u8; 32] = hex::decode(node)
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| "Node key length")?;
    let sig: [u8; 64] = hex::decode(&s.signature_hex)
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| "Node signature length")?;
    ed25519_dalek::VerifyingKey::from_bytes(&key)
        .map_err(|e| e.to_string())?
        .verify_strict(
            &signing_bytes(domain, &(&s.request_digest, &s.payload))?,
            &ed25519_dalek::Signature::from_bytes(&sig),
        )
        .map_err(|e| e.to_string())
}
/// Historical verification only; neither success nor a Known fact is Current.
pub fn verify_stop_record_fact_v2(
    q: &StopRecordControlRequestV2,
    fact: &SignedServiceResponse<StopRecordFactProjectionV2>,
    node: &str,
) -> Result<(), String> {
    strict_node(STOP_RECORD_WITNESS_DOMAIN_V2, q, fact, node)?;
    fact.payload.validate(q)
}
/// Evidence verification, not Account freshCurrent/admit or permission to send.
pub fn verify_stop_record_control_v2(
    q: &StopRecordControlRequestV2,
    response: &StopRecordControlResponseV2,
    node: &str,
) -> Result<(), String> {
    verify_stop_record_fact_v2(q, &response.fact, node)?;
    strict_node(
        STOP_RECORD_CONTROL_CURRENT_DOMAIN_V2,
        q,
        &response.current,
        node,
    )?;
    response.current.payload.validate(q, &response.fact.payload)
}
