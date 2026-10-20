//! Durable cognition markers, receipts, snapshot values, and legacy defaults.
//! Crash-prefix recovery orchestration is owned by Runtime.

use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue, json};

/// The only crash prefixes understood by the v1 recovery oracle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CognitionCrashPrefix {
    BeforePrepared,
    PreparedOnly,
    Committed,
    CommittedMissingProjection,
    Conflict,
}

/// The single durable marker binding an envelope, staged root and receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldCommitRecordV1 {
    pub schema_version: String,
    pub commit_id: String,
    pub envelope_idempotency_key: String,
    pub envelope_digest: String,
    pub world_id: String,
    pub branch_id: String,
    pub finality_epoch: u64,
    #[serde(default)]
    pub finality_block_hash: Option<String>,
    pub finality_status: String,
    pub finality_binding_digest: String,
    pub runtime_manifest_hash: String,
    pub action_id: String,
    pub parent_tick: u64,
    pub parent_world_hash: String,
    pub staged_event_root: String,
    pub staged_state_root: String,
    pub receipt_id: String,
    pub receipt_digest: String,
    pub reorg_epoch: u64,
    pub cognition_journal_seq: u64,
    pub status: String,
    /// Dense identity copied from the validated envelope.  Empty values are
    /// retained only for read-only compatibility with pre-v1 projections;
    /// World-created markers always populate every field.
    #[serde(default)]
    pub agent_id: String,
    #[serde(default)]
    pub agent_session_id: String,
    #[serde(default)]
    pub agent_turn_id: String,
    #[serde(default)]
    pub decision_request_id: String,
    #[serde(default)]
    pub request_digest: String,
    #[serde(default)]
    pub feedback_id: String,
    /// Optional continuous-agent response binding. Legacy markers omit these
    /// fields; the strict World conversion seam fills them before finalize.
    #[serde(default)]
    pub response_context_discriminator: String,
    #[serde(default)]
    pub response_context_version: u16,
    #[serde(default)]
    pub response_retry_seq: u64,
    #[serde(default)]
    pub transport_attempt: u64,
    #[serde(default)]
    pub response_artifact_digest: String,
    #[serde(default)]
    pub abort_reason: Option<String>,
}

/// The externally visible canonical world root.  Recovery must never expose
/// the staged root until the marker says `committed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldRootViewV1 {
    pub schema_version: String,
    pub world_id: String,
    pub branch_id: String,
    pub logical_tick: u64,
    pub state_root: String,
    pub head_status: String,
    #[serde(default)]
    pub commit_id: Option<String>,
    #[serde(default)]
    pub quarantine_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CognitionReceiptViewV1 {
    pub receipt_id: String,
    pub receipt_digest: String,
}

/// Runtime-issued receipt lineage projected to the simulator cognition
/// boundary.  The durable `WorldCommitRecordV1` remains the authority; this
/// projection carries the correlated turn and feedback identity required by
/// an authoritative memory commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeReceiptLineageV1 {
    pub schema_version: String,
    pub status: String,
    pub receipt_id: String,
    pub receipt_digest: String,
    pub envelope_digest: String,
    pub action_id: String,
    pub agent_id: String,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub request_digest: String,
    pub feedback_id: String,
}

pub use super::cognition_feedback_contract::{
    RuntimeFeedbackOutboxRecordV1, RuntimeFeedbackProjectionV1, RuntimeFeedbackRequestV1,
};

pub use oasis7_agent_api::world_cognition::{
    CognitionRecoveryError, RuntimeCognitionBaseBindingV1, RuntimeCognitionCommitRequestV1,
    RuntimeCognitionResponseArtifactV1,
};

impl RuntimeReceiptLineageV1 {
    pub const SCHEMA_VERSION: &'static str = "runtime-receipt-lineage.v1";

    /// Convert this Runtime-owned receipt correlation into the portable API
    /// projection. Callers must still use Runtime's durable readback verifier;
    /// the returned DTO is not a proof of commit or world state.
    #[allow(dead_code)] // Harness adopts the projection in the later adapter slice.
    pub fn agent_api_projection(&self) -> oasis7_agent_api::RuntimeReceiptLineageV1 {
        oasis7_agent_api::RuntimeReceiptLineageV1 {
            schema_version: self.schema_version.clone(),
            status: self.status.clone(),
            receipt_id: self.receipt_id.clone(),
            receipt_digest: self.receipt_digest.clone(),
            envelope_digest: self.envelope_digest.clone(),
            action_id: self.action_id.clone(),
            agent_id: self.agent_id.clone(),
            agent_session_id: self.agent_session_id.clone(),
            agent_turn_id: self.agent_turn_id.clone(),
            decision_request_id: self.decision_request_id.clone(),
            request_digest: self.request_digest.clone(),
            feedback_id: self.feedback_id.clone(),
        }
    }

    /// Construct a projection from an already durable committed Runtime
    /// marker.  Receipt and action IDs are never accepted as caller-only
    /// inputs at this boundary.
    #[allow(clippy::too_many_arguments)]
    pub fn from_commit_record(
        marker: &WorldCommitRecordV1,
        agent_id: impl Into<String>,
        agent_session_id: impl Into<String>,
        agent_turn_id: impl Into<String>,
        decision_request_id: impl Into<String>,
        request_digest: impl Into<String>,
        feedback_id: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION.to_string(),
            status: marker.status.clone(),
            receipt_id: marker.receipt_id.clone(),
            receipt_digest: marker.receipt_digest.clone(),
            envelope_digest: marker.envelope_digest.clone(),
            action_id: marker.action_id.clone(),
            agent_id: agent_id.into(),
            agent_session_id: agent_session_id.into(),
            agent_turn_id: agent_turn_id.into(),
            decision_request_id: decision_request_id.into(),
            request_digest: request_digest.into(),
            feedback_id: feedback_id.into(),
        }
    }

    /// Construct lineage solely from a dense World-owned marker. This is the
    /// projection used by the live runtime boundary; callers do not supply
    /// identity fields independently of the durable commit.
    pub fn from_durable_commit_record(marker: &WorldCommitRecordV1) -> Option<Self> {
        if marker.agent_id.trim().is_empty()
            || marker.agent_session_id.trim().is_empty()
            || marker.agent_turn_id.trim().is_empty()
            || marker.decision_request_id.trim().is_empty()
            || marker.request_digest.trim().is_empty()
            || marker.feedback_id.trim().is_empty()
        {
            return None;
        }
        Some(Self::from_commit_record(
            marker,
            marker.agent_id.clone(),
            marker.agent_session_id.clone(),
            marker.agent_turn_id.clone(),
            marker.decision_request_id.clone(),
            marker.request_digest.clone(),
            marker.feedback_id.clone(),
        ))
    }

    pub fn validate(&self) -> Result<(), CognitionRecoveryError> {
        if self.schema_version != Self::SCHEMA_VERSION
            || self.status != "committed"
            || self.receipt_id.trim().is_empty()
            || self.receipt_digest.trim().is_empty()
            || self.envelope_digest.trim().is_empty()
            || self.action_id.trim().is_empty()
            || self.agent_id.trim().is_empty()
            || self.agent_session_id.trim().is_empty()
            || self.agent_turn_id.trim().is_empty()
            || self.decision_request_id.trim().is_empty()
            || self.request_digest.trim().is_empty()
            || self.feedback_id.trim().is_empty()
        {
            return Err(CognitionRecoveryError::new(
                "runtime_receipt_lineage_invalid",
            ));
        }
        Ok(())
    }
}

/// A response/artifact projection recorded before a process crash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CognitionResponseRecordV1 {
    pub response_digest: String,
    #[serde(default)]
    pub response_artifact: Option<JsonValue>,
    pub envelope_digest: String,
    #[serde(default)]
    pub journal_head: String,
}

pub use oasis7_agent_api::world_cognition::cognition_digest_v1;

pub fn response_artifact_digest(artifact: &JsonValue) -> String {
    cognition_digest_v1("oasis7.cognition.response.v1", artifact)
}

/// The additive cognition projection emitted for worlds written before the
/// cognition protocol existed.  The projection is intentionally explicit:
/// the absence of a durable marker is a read-only compatibility state and can
/// never be interpreted as an effect that is safe to replay.
pub fn default_cognition_persistence_projection() -> JsonValue {
    json!({
        "schema_version": "cognition-persistence.v1",
        "cognition_journal": {
            "schema_version": "cognition-journal.v1",
            "head_seq": 0,
            "head_digest": "",
            "events": []
        },
        "responses": [],
        "commit_records": [],
        "receipt_registry": [],
        "receipt_lineage_registry": [],
        "feedback_outbox": {},
        "idempotency_index": {},
        "staged_actions": {},
        "scheduler_state": null,
        "continuations": [],
        "recovery": {
            "crash_prefix": "legacy",
            "disposition": "rejected",
            "reject_reason": "legacy_no_cognition_proof",
            "world_root": null,
            "candidate_root": null,
            "candidate_receipt": null,
            "receipt": null,
            "provider_invocation_count": 0,
            "kernel_invocation_count": 0,
            "effect_count": 0,
            "debit_count": 0,
            "receipt_count": 0,
            "world_receipt_linked_count": 0,
            "response_replayed": false
        }
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CognitionSnapshotV1 {
    #[serde(default)]
    pub schema_version: String,
    #[serde(default)]
    pub world_id: String,
    #[serde(default)]
    pub legacy_action_id: Option<u64>,
    #[serde(default)]
    pub legacy_success: Option<bool>,
    #[serde(default)]
    pub legacy_summary: Option<String>,
    #[serde(default)]
    pub legacy_no_cognition_proof: bool,
}

impl CognitionSnapshotV1 {
    pub fn from_legacy_json(value: JsonValue) -> Result<Self, CognitionRecoveryError> {
        let object = value
            .as_object()
            .ok_or_else(|| CognitionRecoveryError::new("legacy_snapshot_invalid"))?;
        let schema_version = object
            .get("schema_version")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| CognitionRecoveryError::new("legacy_snapshot_invalid"))?;
        if schema_version != "snapshot.v0" {
            return Err(CognitionRecoveryError::new("unsupported_snapshot"));
        }
        let world_id = object
            .get("world_id")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
            .to_string();
        let queued_action = object
            .get("queued_action")
            .and_then(JsonValue::as_object)
            .ok_or_else(|| CognitionRecoveryError::new("legacy_snapshot_invalid"))?;
        Ok(Self {
            schema_version: schema_version.to_string(),
            world_id,
            legacy_action_id: queued_action.get("action_id").and_then(JsonValue::as_u64),
            legacy_success: queued_action.get("success").and_then(JsonValue::as_bool),
            legacy_summary: queued_action
                .get("summary")
                .and_then(JsonValue::as_str)
                .map(str::to_string),
            legacy_no_cognition_proof: true,
        })
    }
}

/// Portable receipt metadata only; conversion never constructs a commit/readback capability.
impl From<&RuntimeReceiptLineageV1> for oasis7_agent_api::RuntimeReceiptLineageV1 {
    fn from(lineage: &RuntimeReceiptLineageV1) -> Self {
        lineage.agent_api_projection()
    }
}
