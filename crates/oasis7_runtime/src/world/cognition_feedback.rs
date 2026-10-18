//! World-owned durable feedback delivery state.
//!
//! Provider transports are allowed to fail or be restarted. The Runtime keeps
//! the exact feedback envelope and its delivery lease in the cognition
//! projection, so retrying delivery never requires re-running cognition.

use super::World;
use crate::cognition_recovery::{
    RuntimeFeedbackOutboxRecordV1, RuntimeFeedbackProjectionV1, RuntimeFeedbackRequestV1,
    cognition_digest_v1, default_cognition_persistence_projection,
};
use crate::error::WorldError;
use crate::world::cognition_persistence_validation::append_cognition_event;
use oasis7_agent_api::Digest32;
use oasis7_agent_api::FeedbackEnvelopeV1;
use serde_json::{Value as JsonValue, json};
use std::collections::BTreeMap;

const OUTBOX_FIELD: &str = "feedback_outbox";

fn verify_feedback_receipt_lineage(
    world: &World,
    request: &RuntimeFeedbackRequestV1,
    feedback_projection: &RuntimeFeedbackProjectionV1,
) -> Result<Option<String>, WorldError> {
    if request.status != "committed" {
        return Ok(None);
    }
    let receipt_id = request
        .runtime_receipt_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| feedback_error("feedback_receipt_required"))?;
    let lineage = world.read_runtime_receipt_lineage(receipt_id)?;
    if lineage.agent_id != request.agent_subject
        || lineage.agent_session_id != request.agent_session_id
        || lineage.agent_turn_id != request.agent_turn_id
        || lineage.decision_request_id != request.decision_request_id
        || lineage.request_digest != request.request_digest
        || request
            .candidate_action_id
            .is_some_and(|action_id| lineage.action_id != format!("action:{action_id}"))
        || feedback_projection
            .envelope_digest
            .as_deref()
            .is_some_and(|digest| digest != lineage.envelope_digest)
    {
        return Err(feedback_error("feedback_receipt_lineage_mismatch"));
    }
    if request
        .feedback_id
        .as_deref()
        .is_some_and(|feedback_id| feedback_id != lineage.feedback_id)
    {
        return Err(feedback_error("feedback_receipt_lineage_mismatch"));
    }
    Ok(Some(lineage.feedback_id))
}

fn feedback_error(code: impl Into<String>) -> WorldError {
    WorldError::CapabilityAuthorizationDenied {
        reason: code.into(),
    }
}

fn parse_outbox(
    projection: &JsonValue,
) -> Result<BTreeMap<String, RuntimeFeedbackOutboxRecordV1>, WorldError> {
    let Some(value) = projection.get(OUTBOX_FIELD) else {
        return Ok(BTreeMap::new());
    };
    let records: BTreeMap<String, RuntimeFeedbackOutboxRecordV1> =
        serde_json::from_value(value.clone())
            .map_err(|_| feedback_error("feedback_outbox_invalid"))?;
    for (key, record) in &records {
        if key != &record.feedback_id {
            return Err(feedback_error("feedback_outbox_key_mismatch"));
        }
        record
            .validate()
            .map_err(|error| feedback_error(error.code()))?;
    }
    Ok(records)
}

fn current_projection(world: &World) -> JsonValue {
    if world.cognition().is_null() {
        default_cognition_persistence_projection()
    } else {
        world.cognition().clone()
    }
}

fn persist_outbox(
    world: &mut World,
    mut projection: JsonValue,
    records: BTreeMap<String, RuntimeFeedbackOutboxRecordV1>,
    kind: &str,
    record: &RuntimeFeedbackOutboxRecordV1,
) -> Result<(), WorldError> {
    let object = projection
        .as_object_mut()
        .ok_or_else(|| feedback_error("cognition_projection_not_object"))?;
    object.insert(OUTBOX_FIELD.to_string(), serde_json::to_value(records)?);
    append_cognition_event(
        &mut projection,
        kind,
        json!({
            "feedback_id": record.feedback_id,
            "feedback_seq": record.feedback_seq,
            "agent_id": record.agent_subject,
            "agent_session_id": record.agent_session_id,
            "agent_turn_id": record.agent_turn_id,
            "decision_request_id": record.decision_request_id,
            "request_digest": record.request_digest,
            "feedback_digest": record.envelope_digest,
            "feedback_state": record.state,
            "attempt": record.attempt,
            "projection": record.projection,
        }),
    )?;
    world.cognition = projection;
    Ok(())
}

impl World {
    /// Allocate and queue a complete Runtime-authoritative feedback envelope.
    /// Sequence allocation is durable per `(agent_subject, session)` and the
    /// derived id is stable for an exact disposition replay. Adapters never
    /// need to mint feedback identity or sequence values locally.
    pub fn allocate_runtime_feedback(
        &mut self,
        request: RuntimeFeedbackRequestV1,
    ) -> Result<RuntimeFeedbackOutboxRecordV1, WorldError> {
        self.allocate_runtime_feedback_with_projection(
            request,
            RuntimeFeedbackProjectionV1::default(),
        )
    }

    /// Allocate and queue a complete Runtime-authoritative feedback envelope
    /// with its optional bounded cognition/world projection. The projection
    /// is validated in the same identity transaction and cannot alter the
    /// disposition or receipt requirements.
    pub fn allocate_runtime_feedback_with_projection(
        &mut self,
        request: RuntimeFeedbackRequestV1,
        feedback_projection: RuntimeFeedbackProjectionV1,
    ) -> Result<RuntimeFeedbackOutboxRecordV1, WorldError> {
        let projection = current_projection(self);
        let records = parse_outbox(&projection)?;
        let receipt_feedback_id =
            verify_feedback_receipt_lineage(self, &request, &feedback_projection)?;
        let requested_id = request.feedback_id.clone().or(receipt_feedback_id);
        let feedback_id = requested_id.unwrap_or_else(|| {
            cognition_digest_v1(
                "oasis7.runtime.feedback-id.v1",
                &json!({
                    "agent_subject": &request.agent_subject,
                    "agent_session_id": &request.agent_session_id,
                    "agent_turn_id": &request.agent_turn_id,
                    "decision_request_id": &request.decision_request_id,
                    "candidate_action_id": request.candidate_action_id,
                    "runtime_receipt_id": &request.runtime_receipt_id,
                    "status": &request.status,
                    "request_digest": &request.request_digest,
                    "reject_reason": &request.reject_reason,
                }),
            )
        });
        let feedback_seq = if let Some(existing) = records.get(&feedback_id) {
            existing.feedback_seq
        } else {
            records
                .values()
                .filter(|existing| {
                    existing.agent_subject == request.agent_subject
                        && existing.agent_session_id == request.agent_session_id
                })
                .map(|existing| existing.feedback_seq)
                .max()
                .unwrap_or_default()
                .checked_add(1)
                .ok_or_else(|| feedback_error("feedback_sequence_overflow"))?
        };
        let feedback = FeedbackEnvelopeV1 {
            feedback_id,
            feedback_seq,
            agent_subject: request.agent_subject,
            agent_session_id: request.agent_session_id,
            agent_turn_id: request.agent_turn_id,
            decision_request_id: request.decision_request_id,
            candidate_action_id: request.candidate_action_id,
            runtime_receipt_id: request.runtime_receipt_id,
            status: request.status,
            request_digest: Digest32::from(request.request_digest),
            reject_reason: request.reject_reason,
            provenance: "runtime_authoritative".to_string(),
        };
        self.enqueue_runtime_feedback_with_projection(feedback, feedback_projection)
    }

    /// Queue one Runtime-authoritative feedback envelope. Re-enqueueing the
    /// exact canonical envelope is idempotent; reusing its identity with any
    /// changed field is rejected before the projection is touched.
    pub fn enqueue_runtime_feedback(
        &mut self,
        feedback: FeedbackEnvelopeV1,
    ) -> Result<RuntimeFeedbackOutboxRecordV1, WorldError> {
        self.enqueue_runtime_feedback_with_projection(
            feedback,
            RuntimeFeedbackProjectionV1::default(),
        )
    }

    fn enqueue_runtime_feedback_with_projection(
        &mut self,
        feedback: FeedbackEnvelopeV1,
        feedback_projection: RuntimeFeedbackProjectionV1,
    ) -> Result<RuntimeFeedbackOutboxRecordV1, WorldError> {
        let record = RuntimeFeedbackOutboxRecordV1::from_feedback_with_projection(
            &feedback,
            feedback_projection,
        )
        .map_err(|error| feedback_error(error.code()))?;
        let projection = current_projection(self);
        let mut records = parse_outbox(&projection)?;
        if let Some(existing) = records.get(&record.feedback_id) {
            if existing.envelope_digest != record.envelope_digest
                || existing.projection != record.projection
            {
                return Err(feedback_error("feedback_identity_collision"));
            }
            return Ok(existing.clone());
        }
        if records.values().any(|existing| {
            existing.agent_subject == record.agent_subject
                && existing.agent_session_id == record.agent_session_id
                && existing.feedback_seq == record.feedback_seq
        }) {
            return Err(feedback_error("feedback_sequence_collision"));
        }
        records.insert(record.feedback_id.clone(), record.clone());
        persist_outbox(self, projection, records, "ProviderFeedbackQueued", &record)?;
        Ok(record)
    }

    /// Return every durable feedback record, including acknowledged records
    /// retained for idempotent replay/collision detection.
    pub fn runtime_feedback_outbox(
        &self,
    ) -> Result<Vec<RuntimeFeedbackOutboxRecordV1>, WorldError> {
        Ok(parse_outbox(&current_projection(self))?
            .into_values()
            .collect())
    }

    /// Return pending records in canonical feedback-id order for a transport
    /// worker. Runtime never invokes a provider while claiming a lease.
    pub fn pending_runtime_feedback(
        &self,
    ) -> Result<Vec<RuntimeFeedbackOutboxRecordV1>, WorldError> {
        let mut records = self
            .runtime_feedback_outbox()?
            .into_iter()
            .filter(|record| record.state == "pending")
            .collect::<Vec<_>>();
        records.sort_by(|left, right| {
            (
                &left.agent_subject,
                &left.agent_session_id,
                left.feedback_seq,
                &left.feedback_id,
            )
                .cmp(&(
                    &right.agent_subject,
                    &right.agent_session_id,
                    right.feedback_seq,
                    &right.feedback_id,
                ))
        });
        Ok(records)
    }

    /// Claim one pending record for delivery. A repeated claim of an existing
    /// lease is idempotent and does not increment the attempt counter.
    pub fn claim_runtime_feedback(
        &mut self,
        feedback_id: &str,
    ) -> Result<RuntimeFeedbackOutboxRecordV1, WorldError> {
        let projection = current_projection(self);
        let mut records = parse_outbox(&projection)?;
        let record = records
            .get_mut(feedback_id)
            .ok_or_else(|| feedback_error("feedback_outbox_missing"))?;
        if record.state == "acked" {
            return Err(feedback_error("feedback_already_acked"));
        }
        if record.state == "in_flight" {
            return Ok(record.clone());
        }
        if record.state == "pending" {
            record.state = "in_flight".to_string();
            record.attempt = record.attempt.saturating_add(1);
            record.last_error = None;
        }
        let result = record.clone();
        persist_outbox(
            self,
            projection,
            records,
            "ProviderFeedbackClaimed",
            &result,
        )?;
        Ok(result)
    }

    /// Return a failed delivery lease to pending without losing its attempt
    /// count. The transport can retry later, including after a restart.
    pub fn retry_runtime_feedback(
        &mut self,
        feedback_id: &str,
        reason: impl Into<String>,
    ) -> Result<RuntimeFeedbackOutboxRecordV1, WorldError> {
        let projection = current_projection(self);
        let mut records = parse_outbox(&projection)?;
        let record = records
            .get_mut(feedback_id)
            .ok_or_else(|| feedback_error("feedback_outbox_missing"))?;
        if record.state == "acked" {
            return Err(feedback_error("feedback_already_acked"));
        }
        record.state = "pending".to_string();
        let mut reason = reason.into();
        reason.truncate(256);
        record.last_error = (!reason.is_empty()).then_some(reason);
        let result = record.clone();
        persist_outbox(
            self,
            projection,
            records,
            "ProviderFeedbackRetryable",
            &result,
        )?;
        Ok(result)
    }

    /// Mark a claimed envelope delivered. Keeping the terminal record makes
    /// transport retries idempotent and prevents identity reuse.
    pub fn ack_runtime_feedback(&mut self, feedback_id: &str) -> Result<(), WorldError> {
        let projection = current_projection(self);
        let mut records = parse_outbox(&projection)?;
        let record = records
            .get_mut(feedback_id)
            .ok_or_else(|| feedback_error("feedback_outbox_missing"))?;
        if record.state == "acked" {
            return Ok(());
        }
        if record.state != "in_flight" {
            return Err(feedback_error("feedback_not_in_flight"));
        }
        record.state = "acked".to_string();
        record.last_error = None;
        let result = record.clone();
        persist_outbox(
            self,
            projection,
            records,
            "ProviderFeedbackAcknowledged",
            &result,
        )?;
        Ok(())
    }

    /// Recover a process-crash lease. No provider is called and no envelope is
    /// discarded; every in-flight item becomes pending exactly once.
    pub(super) fn recover_feedback_outbox_projection(
        projection: &mut JsonValue,
    ) -> Result<bool, WorldError> {
        let mut records = parse_outbox(projection)?;
        let mut recovered = Vec::new();
        for record in records.values_mut() {
            if record.state == "in_flight" {
                record.state = "pending".to_string();
                record.last_error = Some("recovered_after_restart".to_string());
                recovered.push(record.clone());
            }
        }
        if recovered.is_empty() {
            return Ok(false);
        }
        projection[OUTBOX_FIELD] = serde_json::to_value(records)?;
        for record in recovered {
            append_cognition_event(
                projection,
                "ProviderFeedbackRecovered",
                json!({
                    "feedback_id": record.feedback_id,
                    "feedback_seq": record.feedback_seq,
                    "feedback_digest": record.envelope_digest,
                    "feedback_state": record.state,
                    "attempt": record.attempt,
                }),
            )?;
        }
        Ok(true)
    }
}
