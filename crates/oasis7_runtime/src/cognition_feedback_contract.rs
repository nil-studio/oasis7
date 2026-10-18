//! Runtime-owned feedback contract and durable outbox record.
//!
//! This module is intentionally separate from the recovery oracle so the
//! contract can evolve without pushing the recovery implementation past the
//! repository's per-file size gate.

use super::cognition_recovery::{CognitionRecoveryError, cognition_digest_v1};
use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue, json};

/// Durable Runtime-owned delivery record for provider feedback. The payload is
/// retained as canonical JSON so adapters can transport the exact envelope,
/// while the identity and retry state remain under World authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeFeedbackOutboxRecordV1 {
    pub schema_version: String,
    pub feedback_id: String,
    pub feedback_seq: u64,
    pub agent_subject: String,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub request_digest: String,
    pub envelope_digest: String,
    /// Optional Runtime-validated projection for the associated cognition
    /// envelope and committed world summary. This is distinct from
    /// `envelope_digest`, which is the canonical digest of the feedback
    /// payload itself.
    #[serde(default)]
    pub projection: RuntimeFeedbackProjectionV1,
    pub payload: JsonValue,
    pub state: String,
    pub attempt: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

/// Runtime-owned feedback disposition input. Callers provide only the
/// verified turn identity and terminal disposition; Runtime allocates the
/// per-session feedback sequence and canonical feedback id before queuing the
/// complete envelope in its durable outbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeFeedbackRequestV1 {
    /// A Runtime-issued id may be supplied when a receipt lineage already
    /// carries one. Otherwise Runtime derives a deterministic id from the
    /// complete disposition identity.
    #[serde(default)]
    pub feedback_id: Option<String>,
    pub agent_subject: String,
    pub agent_session_id: String,
    pub agent_turn_id: String,
    pub decision_request_id: String,
    pub candidate_action_id: Option<u64>,
    pub runtime_receipt_id: Option<String>,
    pub status: String,
    pub request_digest: String,
    pub reject_reason: Option<String>,
}

/// Optional, Runtime-validated projection accompanying a disposition. The
/// projection is deliberately separate from the status/receipt identity: a
/// provider or Viewer may not use a summary to manufacture a disposition.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeFeedbackProjectionV1 {
    #[serde(default)]
    pub envelope_digest: Option<String>,
    /// Runtime-approved, bounded summaries of the committed events.
    #[serde(default)]
    pub emitted_events: Vec<JsonValue>,
    #[serde(default)]
    pub committed_event_summary: Option<String>,
    #[serde(default)]
    pub world_delta_summary: Option<String>,
}

impl RuntimeFeedbackProjectionV1 {
    pub fn validate(&self) -> Result<(), CognitionRecoveryError> {
        if self.emitted_events.len() > 32
            || self.emitted_events.iter().any(|event| {
                let encoded = serde_json::to_vec(event).unwrap_or_default();
                encoded.len() > 16 * 1024 || contains_sensitive_marker(&encoded)
            })
            || self
                .envelope_digest
                .as_deref()
                .is_some_and(|value| !canonical_blake3(value))
            || self
                .committed_event_summary
                .as_deref()
                .is_some_and(|value| {
                    value.len() > 512 || contains_sensitive_marker(value.as_bytes())
                })
            || self.world_delta_summary.as_deref().is_some_and(|value| {
                value.len() > 1024 || contains_sensitive_marker(value.as_bytes())
            })
        {
            return Err(CognitionRecoveryError::new("feedback_projection_too_large"));
        }
        let encoded = serde_json::to_vec(self)
            .map_err(|_| CognitionRecoveryError::new("feedback_projection_invalid"))?;
        if encoded.len() > 16 * 1024 {
            return Err(CognitionRecoveryError::new("feedback_projection_too_large"));
        }
        Ok(())
    }
}

fn contains_sensitive_marker(value: &[u8]) -> bool {
    let value = String::from_utf8_lossy(value).to_ascii_lowercase();
    [
        "credential",
        "token",
        "authorization",
        "private-key",
        "private_key",
        "path",
        "bearer ",
        "sk-",
    ]
    .iter()
    .any(|marker| value.contains(marker))
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

/// Stable Runtime disposition-to-feedback mapping. The adapter cannot invent
/// a status-specific reason, receipt, or action projection and then label it
/// `runtime_authoritative`.
pub(crate) fn validate_runtime_feedback_disposition(
    status: &str,
    candidate_action_id: Option<u64>,
    runtime_receipt_id: Option<&str>,
    reject_reason: Option<&str>,
    projection: &RuntimeFeedbackProjectionV1,
) -> Result<(), CognitionRecoveryError> {
    const REJECTED_REASONS: &[&str] = &[
        "stale_base",
        "expired",
        "stale_capability_snapshot",
        "authority_denied",
        "intent_conflict",
        "reorg_invalidated",
        "finality_anchor_mismatch",
        "precondition_failed",
        "action_rejected",
        "idempotency_conflict",
        "no_effect",
        "cancelled",
        "late_response_after_cancel",
        "legacy_no_cognition_proof",
        "cognition_context_mismatch",
    ];
    const FAILED_REASONS: &[&str] = &[
        "failed_provider",
        "failed_persist",
        "cognition_failed",
        "provider_unavailable",
    ];
    const PENDING_REASONS: &[&str] = &[
        "recovery_pending",
        "retry_scheduled",
        "scheduler_backpressure",
    ];
    let reason = reject_reason.map(str::trim);
    let no_receipt = runtime_receipt_id.is_none_or(str::is_empty);
    match status {
        "committed" if candidate_action_id.is_some() && !no_receipt && reason.is_none() => {}
        "rejected" if candidate_action_id.is_none() && no_receipt => {
            if !reason.is_some_and(|reason| REJECTED_REASONS.contains(&reason)) {
                return Err(CognitionRecoveryError::new(
                    "feedback_disposition_reason_invalid",
                ));
            }
        }
        "failed" if candidate_action_id.is_none() && no_receipt => {
            if !reason.is_some_and(|reason| FAILED_REASONS.contains(&reason)) {
                return Err(CognitionRecoveryError::new(
                    "feedback_disposition_reason_invalid",
                ));
            }
        }
        "pending" if candidate_action_id.is_none() && no_receipt => {
            if let Some(reason) = reason
                && !PENDING_REASONS.contains(&reason)
            {
                return Err(CognitionRecoveryError::new(
                    "feedback_disposition_reason_invalid",
                ));
            }
        }
        _ => {
            return Err(CognitionRecoveryError::new(
                "feedback_disposition_inconsistent",
            ));
        }
    }
    projection.validate()
}

/// Add Runtime-owned projection fields to the canonical wire payload. The
/// legacy `FeedbackEnvelopeV1` DTO remains source-compatible; adapters that
/// understand the target contract should transport this JSON value rather
/// than the base DTO serialization.
fn feedback_payload_with_projection(
    payload: &JsonValue,
    projection: &RuntimeFeedbackProjectionV1,
) -> Result<JsonValue, CognitionRecoveryError> {
    projection.validate()?;
    let mut payload = payload.clone();
    let object = payload
        .as_object_mut()
        .ok_or_else(|| CognitionRecoveryError::new("runtime_feedback_payload_invalid"))?;
    let insert_if_absent_or_equal = |object: &mut serde_json::Map<String, JsonValue>,
                                     key: &str,
                                     value: JsonValue|
     -> Result<(), CognitionRecoveryError> {
        if let Some(existing) = object.get(key) {
            if existing != &value {
                return Err(CognitionRecoveryError::new(
                    "feedback_projection_identity_conflict",
                ));
            }
        } else {
            object.insert(key.to_string(), value);
        }
        Ok(())
    };
    if let Some(value) = projection.envelope_digest.as_deref() {
        insert_if_absent_or_equal(object, "envelope_digest", json!(value))?;
    }
    if !projection.emitted_events.is_empty() {
        insert_if_absent_or_equal(
            object,
            "emitted_events",
            JsonValue::Array(projection.emitted_events.clone()),
        )?;
    }
    if let Some(value) = projection.committed_event_summary.as_deref() {
        insert_if_absent_or_equal(object, "committed_event_summary", json!(value))?;
    }
    if let Some(value) = projection.world_delta_summary.as_deref() {
        insert_if_absent_or_equal(object, "world_delta_summary", json!(value))?;
    }
    let encoded = serde_json::to_vec(&payload)
        .map_err(|_| CognitionRecoveryError::new("runtime_feedback_payload_invalid"))?;
    if encoded.len() > 16 * 1024 {
        return Err(CognitionRecoveryError::new("feedback_projection_too_large"));
    }
    Ok(payload)
}

impl RuntimeFeedbackOutboxRecordV1 {
    pub const SCHEMA_VERSION: &'static str = "runtime-feedback-outbox.v1";

    pub fn validate(&self) -> Result<(), CognitionRecoveryError> {
        let bounded = |value: &str| !value.trim().is_empty() && value.len() <= 256;
        let valid_state = matches!(self.state.as_str(), "pending" | "in_flight" | "acked");
        let payload =
            serde_json::from_value::<oasis7_agent_api::FeedbackEnvelopeV1>(self.payload.clone())
                .map_err(|_| CognitionRecoveryError::new("runtime_feedback_payload_invalid"))?;
        let canonical_base_payload = serde_json::to_value(&payload)
            .map_err(|_| CognitionRecoveryError::new("runtime_feedback_payload_invalid"))?;
        let canonical_payload =
            feedback_payload_with_projection(&canonical_base_payload, &self.projection)?;
        let envelope_digest =
            cognition_digest_v1("oasis7.runtime.feedback-envelope.v1", &canonical_payload);
        let feedback_contract_valid = payload.provenance == "runtime_authoritative"
            && matches!(
                payload.status.as_str(),
                "pending" | "committed" | "rejected" | "failed"
            )
            && validate_runtime_feedback_disposition(
                payload.status.as_str(),
                payload.candidate_action_id,
                payload.runtime_receipt_id.as_deref(),
                payload.reject_reason.as_deref(),
                &self.projection,
            )
            .is_ok();
        if self.schema_version != Self::SCHEMA_VERSION
            || !bounded(&self.feedback_id)
            || self.feedback_seq == 0
            || !bounded(&self.agent_subject)
            || !bounded(&self.agent_session_id)
            || !bounded(&self.agent_turn_id)
            || !bounded(&self.decision_request_id)
            || !canonical_blake3(&self.request_digest)
            || self.envelope_digest != envelope_digest
            || !feedback_contract_valid
            || !valid_state
            || self.payload != canonical_payload
            || payload.feedback_id != self.feedback_id
            || payload.feedback_seq != self.feedback_seq
            || payload.agent_subject != self.agent_subject
            || payload.agent_session_id != self.agent_session_id
            || payload.agent_turn_id != self.agent_turn_id
            || payload.decision_request_id != self.decision_request_id
            || payload.request_digest.to_string() != self.request_digest
        {
            return Err(CognitionRecoveryError::new(
                "runtime_feedback_outbox_record_invalid",
            ));
        }
        Ok(())
    }

    pub(crate) fn from_feedback_with_projection(
        feedback: &oasis7_agent_api::FeedbackEnvelopeV1,
        projection: RuntimeFeedbackProjectionV1,
    ) -> Result<Self, CognitionRecoveryError> {
        let payload = serde_json::to_value(feedback)
            .map_err(|_| CognitionRecoveryError::new("runtime_feedback_payload_invalid"))?;
        if feedback.feedback_id.trim().is_empty()
            || feedback.feedback_seq == 0
            || feedback.agent_subject.trim().is_empty()
            || feedback.agent_session_id.trim().is_empty()
            || feedback.agent_turn_id.trim().is_empty()
            || feedback.decision_request_id.trim().is_empty()
            || !feedback.request_digest.is_canonical_blake3()
            || feedback.provenance != "runtime_authoritative"
            || !matches!(
                feedback.status.as_str(),
                "pending" | "committed" | "rejected" | "failed"
            )
        {
            return Err(CognitionRecoveryError::new(
                "runtime_feedback_outbox_feedback_invalid",
            ));
        }
        validate_runtime_feedback_disposition(
            feedback.status.as_str(),
            feedback.candidate_action_id,
            feedback.runtime_receipt_id.as_deref(),
            feedback.reject_reason.as_deref(),
            &projection,
        )?;
        let payload = feedback_payload_with_projection(&payload, &projection)?;
        let envelope_digest = cognition_digest_v1("oasis7.runtime.feedback-envelope.v1", &payload);
        let record = Self {
            schema_version: Self::SCHEMA_VERSION.to_string(),
            feedback_id: feedback.feedback_id.clone(),
            feedback_seq: feedback.feedback_seq,
            agent_subject: feedback.agent_subject.clone(),
            agent_session_id: feedback.agent_session_id.clone(),
            agent_turn_id: feedback.agent_turn_id.clone(),
            decision_request_id: feedback.decision_request_id.clone(),
            request_digest: feedback.request_digest.to_string(),
            envelope_digest,
            projection,
            payload,
            state: "pending".to_string(),
            attempt: 0,
            last_error: None,
        };
        record.validate()?;
        Ok(record)
    }

    /// Return the complete canonical JSON payload for an adapter transport.
    /// This includes optional projection fields and revalidates the record so
    /// a stale or tampered outbox entry cannot cross the Runtime boundary.
    pub fn transport_payload(&self) -> Result<JsonValue, CognitionRecoveryError> {
        self.validate()?;
        Ok(self.payload.clone())
    }
}
