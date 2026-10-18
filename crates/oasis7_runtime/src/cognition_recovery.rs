//! Durable cognition commit/recovery seam.
//!
//! The real world store can adopt these records incrementally.  This module
//! deliberately models the authoritative marker and its crash-prefix oracle
//! without rewriting the existing `Journal` format or invoking a provider.

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
    pub(crate) fn agent_api_projection(&self) -> oasis7_agent_api::RuntimeReceiptLineageV1 {
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

pub(crate) use oasis7_agent_api::world_cognition::cognition_digest_v1;

pub(crate) fn response_artifact_digest(artifact: &JsonValue) -> String {
    cognition_digest_v1("oasis7.cognition.response.v1", artifact)
}

/// Fault-injection fixture used by recovery tests and future storage tests.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CognitionRecoveryFixture {
    pub prefix: CognitionCrashPrefix,
    pub world_root: WorldRootViewV1,
    pub commit_record: WorldCommitRecordV1,
    #[serde(default)]
    pub response: Option<CognitionResponseRecordV1>,
    #[serde(default = "default_true")]
    pub receipt_projection_present: bool,
    #[serde(default = "default_true")]
    pub idempotency_projection_present: bool,
    #[serde(default = "default_true")]
    pub world_receipt_linked: bool,
    #[serde(default)]
    pub conflict: Option<String>,
}

fn default_true() -> bool {
    true
}

impl CognitionRecoveryFixture {
    pub fn new(
        prefix: CognitionCrashPrefix,
        world_root: WorldRootViewV1,
        commit_record: WorldCommitRecordV1,
    ) -> Self {
        let committed = matches!(
            prefix,
            CognitionCrashPrefix::Committed | CognitionCrashPrefix::CommittedMissingProjection
        );
        Self {
            prefix,
            world_root,
            commit_record,
            response: None,
            receipt_projection_present: committed,
            idempotency_projection_present: committed,
            world_receipt_linked: committed,
            conflict: None,
        }
    }

    pub fn committed(
        world_root: WorldRootViewV1,
        commit_record: WorldCommitRecordV1,
        response: JsonValue,
    ) -> Self {
        let response = serde_json::from_value(response).unwrap_or_else(|_| {
            // The response fixture is intentionally a small JSON object.  If
            // an older caller omits optional fields, preserve it as an opaque
            // artifact while retaining the marker's digest binding.
            CognitionResponseRecordV1 {
                response_digest: String::new(),
                response_artifact: None,
                envelope_digest: commit_record.envelope_digest.clone(),
                journal_head: String::new(),
            }
        });
        Self {
            prefix: CognitionCrashPrefix::Committed,
            world_root,
            commit_record,
            response: Some(response),
            receipt_projection_present: true,
            idempotency_projection_present: true,
            world_receipt_linked: true,
            conflict: None,
        }
    }
}

/// Counters supplied by tests/storage fault injectors.  Recovery never calls
/// the provider or kernel, so these values remain unchanged by `recover`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CognitionRecoveryProbe {
    pub provider_invocation_count: u64,
    pub kernel_invocation_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CognitionRecoveryReport {
    pub world_root: Option<WorldRootViewV1>,
    pub receipt: Option<CognitionReceiptViewV1>,
    pub disposition: String,
    #[serde(default)]
    pub reject_reason: Option<String>,
    pub auto_submitted: bool,
    pub idempotency_key: Option<String>,
    pub quarantine_id: Option<String>,
    pub candidate_root: Option<String>,
    pub candidate_receipt: Option<CognitionReceiptViewV1>,
    pub journal_head: String,
    pub retry_count: u64,
    pub revalidation_count: u64,
    pub projection_repairs: u64,
    pub provider_invocation_count: u64,
    pub kernel_invocation_count: u64,
    pub effect_count: u64,
    pub debit_count: u64,
    pub world_receipt_linked_count: u64,
    pub event_count: u64,
    pub response_replayed: bool,
}

pub struct CognitionRecovery;

/// The additive cognition projection emitted for worlds written before the
/// cognition protocol existed.  The projection is intentionally explicit:
/// the absence of a durable marker is a read-only compatibility state and can
/// never be interpreted as an effect that is safe to replay.
pub(crate) fn default_cognition_persistence_projection() -> JsonValue {
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

impl CognitionRecovery {
    /// Recover one fixture according to the v1 marker protocol.
    pub fn recover(
        fixture: &mut CognitionRecoveryFixture,
        _probe: &mut CognitionRecoveryProbe,
    ) -> Result<CognitionRecoveryReport, CognitionRecoveryError> {
        validate_marker(&fixture.commit_record)?;
        if let Some(conflict) = fixture.conflict.as_deref() {
            return Ok(conflict_report(fixture, conflict));
        }
        if marker_binding_conflict(fixture) {
            return Ok(conflict_report(fixture, "marker_or_root_mismatch"));
        }
        if fixture.response.as_ref().is_some_and(|response| {
            response.envelope_digest != fixture.commit_record.envelope_digest
        }) {
            return Ok(conflict_report(fixture, "response_envelope_mismatch"));
        }

        // An aborted marker is a durable terminal decision. It preserves the
        // canonical parent root and closes the turn without fabricating a
        // receipt or reopening the request, regardless of which crash-prefix
        // fixture was used to capture the marker.
        if fixture.commit_record.status == "aborted" {
            return Ok(aborted_report(fixture));
        }

        match fixture.prefix {
            CognitionCrashPrefix::BeforePrepared | CognitionCrashPrefix::PreparedOnly => {
                if fixture.commit_record.status != "prepared" {
                    return Ok(conflict_report(fixture, "marker_status_mismatch"));
                }
                Ok(pending_report(fixture))
            }
            CognitionCrashPrefix::Committed | CognitionCrashPrefix::CommittedMissingProjection => {
                if fixture.commit_record.status != "committed" {
                    return Ok(conflict_report(fixture, "marker_status_mismatch"));
                }
                Ok(committed_report(fixture))
            }
            CognitionCrashPrefix::Conflict => Ok(conflict_report(fixture, "marker_mismatch")),
        }
    }

    /// Restore an additive snapshot.  Legacy success has no cognition marker
    /// or canonical receipt and therefore cannot be auto-submitted.
    pub fn restore_snapshot(
        snapshot: CognitionSnapshotV1,
    ) -> Result<CognitionRecoveryReport, CognitionRecoveryError> {
        if !snapshot.legacy_no_cognition_proof {
            return Err(CognitionRecoveryError::new("unsupported_snapshot"));
        }
        Ok(CognitionRecoveryReport {
            world_root: None,
            receipt: None,
            disposition: "rejected".to_string(),
            reject_reason: Some("legacy_no_cognition_proof".to_string()),
            auto_submitted: false,
            idempotency_key: None,
            quarantine_id: None,
            candidate_root: None,
            candidate_receipt: None,
            journal_head: String::new(),
            retry_count: 0,
            revalidation_count: 0,
            projection_repairs: 0,
            provider_invocation_count: 0,
            kernel_invocation_count: 0,
            effect_count: 0,
            debit_count: 0,
            world_receipt_linked_count: 0,
            event_count: 0,
            response_replayed: false,
        })
    }
}

fn validate_marker(marker: &WorldCommitRecordV1) -> Result<(), CognitionRecoveryError> {
    let valid_abort_reason = marker.abort_reason.as_deref().is_some_and(|reason| {
        matches!(
            reason,
            "stale_base"
                | "cancelled"
                | "late_response_after_cancel"
                | "recovery_operator_abort"
                | "reorg_invalidated"
        )
    });
    let abort_shape_valid = match marker.status.as_str() {
        "aborted" => valid_abort_reason,
        "prepared" | "committed" => marker.abort_reason.is_none(),
        _ => false,
    };
    if marker.schema_version != "world-commit-record.v1"
        || marker.commit_id.trim().is_empty()
        || marker.envelope_idempotency_key.trim().is_empty()
        || marker.envelope_digest.trim().is_empty()
        || marker.world_id.trim().is_empty()
        || marker.branch_id.trim().is_empty()
        || marker.parent_world_hash.trim().is_empty()
        || marker.staged_state_root.trim().is_empty()
        || marker.receipt_id.trim().is_empty()
        || marker.receipt_digest.trim().is_empty()
        || !abort_shape_valid
    {
        return Err(CognitionRecoveryError::new("invalid_commit_record"));
    }
    Ok(())
}

fn aborted_report(fixture: &CognitionRecoveryFixture) -> CognitionRecoveryReport {
    let mut root = fixture.world_root.clone();
    root.head_status = "canonical".to_string();
    root.commit_id = None;
    root.quarantine_id = None;
    CognitionRecoveryReport {
        world_root: Some(root),
        receipt: None,
        disposition: "aborted".to_string(),
        reject_reason: fixture.commit_record.abort_reason.clone(),
        auto_submitted: false,
        idempotency_key: Some(fixture.commit_record.envelope_idempotency_key.clone()),
        quarantine_id: None,
        candidate_root: None,
        candidate_receipt: None,
        journal_head: String::new(),
        retry_count: 0,
        revalidation_count: 0,
        projection_repairs: 0,
        provider_invocation_count: 0,
        kernel_invocation_count: 0,
        effect_count: 0,
        debit_count: 0,
        world_receipt_linked_count: 0,
        event_count: 0,
        response_replayed: false,
    }
}

fn marker_binding_conflict(fixture: &CognitionRecoveryFixture) -> bool {
    let marker = &fixture.commit_record;
    let root = &fixture.world_root;
    if root.world_id != marker.world_id || root.branch_id != marker.branch_id {
        return true;
    }
    let parent = root.state_root == marker.parent_world_hash
        && root.logical_tick == marker.parent_tick
        && root.commit_id.is_none();
    let next = root.state_root == marker.staged_state_root
        && root.logical_tick == marker.parent_tick.saturating_add(1)
        && root.commit_id.as_deref() == Some(marker.commit_id.as_str());
    !parent && !next
}

fn pending_report(fixture: &CognitionRecoveryFixture) -> CognitionRecoveryReport {
    let mut root = fixture.world_root.clone();
    root.head_status = "recovery_pending".to_string();
    root.commit_id = None;
    root.quarantine_id = None;
    CognitionRecoveryReport {
        world_root: Some(root),
        receipt: None,
        disposition: "recovery_pending".to_string(),
        reject_reason: None,
        auto_submitted: false,
        idempotency_key: None,
        quarantine_id: None,
        candidate_root: None,
        candidate_receipt: None,
        journal_head: String::new(),
        retry_count: 0,
        revalidation_count: 1,
        projection_repairs: 0,
        provider_invocation_count: 0,
        kernel_invocation_count: 0,
        effect_count: 0,
        debit_count: 0,
        world_receipt_linked_count: 0,
        event_count: 0,
        response_replayed: false,
    }
}

fn committed_report(fixture: &mut CognitionRecoveryFixture) -> CognitionRecoveryReport {
    let marker = &fixture.commit_record;
    let receipt = CognitionReceiptViewV1 {
        receipt_id: marker.receipt_id.clone(),
        receipt_digest: marker.receipt_digest.clone(),
    };
    let missing_projection_count = [
        !fixture.receipt_projection_present,
        !fixture.idempotency_projection_present,
        !fixture.world_receipt_linked,
    ]
    .into_iter()
    .filter(|missing| *missing)
    .count() as u64;
    fixture.receipt_projection_present = true;
    fixture.idempotency_projection_present = true;
    fixture.world_receipt_linked = true;

    let mut root = fixture.world_root.clone();
    root.logical_tick = marker.parent_tick.saturating_add(1);
    root.state_root = marker.staged_state_root.clone();
    root.head_status = "canonical".to_string();
    root.commit_id = Some(marker.commit_id.clone());
    root.quarantine_id = None;
    let journal_head = fixture
        .response
        .as_ref()
        .map(|response| response.journal_head.clone())
        .unwrap_or_default();
    CognitionRecoveryReport {
        world_root: Some(root),
        receipt: Some(receipt.clone()),
        disposition: "committed".to_string(),
        reject_reason: None,
        auto_submitted: false,
        idempotency_key: Some(marker.envelope_idempotency_key.clone()),
        quarantine_id: None,
        candidate_root: None,
        candidate_receipt: None,
        journal_head,
        retry_count: 0,
        revalidation_count: 0,
        projection_repairs: missing_projection_count,
        provider_invocation_count: 0,
        kernel_invocation_count: 0,
        effect_count: u64::from(missing_projection_count == 0),
        debit_count: u64::from(missing_projection_count == 0),
        world_receipt_linked_count: 1,
        event_count: 1,
        response_replayed: fixture.response.is_some(),
    }
}

fn conflict_report(fixture: &CognitionRecoveryFixture, _conflict: &str) -> CognitionRecoveryReport {
    let marker = &fixture.commit_record;
    let quarantine_id = format!("quarantine:{}", marker.commit_id);
    let mut root = fixture.world_root.clone();
    root.head_status = "recovery_pending".to_string();
    root.commit_id = None;
    root.quarantine_id = Some(quarantine_id.clone());
    CognitionRecoveryReport {
        world_root: Some(root),
        receipt: None,
        disposition: "recovery_pending".to_string(),
        reject_reason: Some("commit_conflict".to_string()),
        auto_submitted: false,
        idempotency_key: None,
        quarantine_id: Some(quarantine_id),
        candidate_root: Some(marker.staged_state_root.clone()),
        candidate_receipt: Some(CognitionReceiptViewV1 {
            receipt_id: marker.receipt_id.clone(),
            receipt_digest: marker.receipt_digest.clone(),
        }),
        journal_head: String::new(),
        retry_count: 0,
        revalidation_count: 0,
        projection_repairs: 0,
        provider_invocation_count: 0,
        kernel_invocation_count: 0,
        effect_count: 0,
        debit_count: 0,
        world_receipt_linked_count: 0,
        event_count: 0,
        response_replayed: false,
    }
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
    fn from(lineage: &RuntimeReceiptLineageV1) -> Self { lineage.agent_api_projection() }
}
