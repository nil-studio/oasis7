//! Runtime-owned reorg invalidation and stale-epoch fencing.

use super::super::cognition_persistence_validation::append_cognition_event;
use super::World;
use crate::cognition_wake::{
    ContinuationReorgReport, ContinuationStatusV1, ContinuationTransition,
};
use crate::error::WorldError;
use serde_json::{Value as JsonValue, json};

fn cognition_validation_error(code: &str) -> WorldError {
    WorldError::DistributedValidationFailed {
        reason: format!("cognition validation failed: {code}"),
    }
}

fn scheduler_error(code: &str) -> WorldError {
    WorldError::DistributedValidationFailed {
        reason: format!("cognition scheduler failed: {code}"),
    }
}

fn terminal(status: ContinuationStatusV1) -> bool {
    matches!(
        status,
        ContinuationStatusV1::Completed
            | ContinuationStatusV1::Cancelled
            | ContinuationStatusV1::Invalidated
            | ContinuationStatusV1::Expired
            | ContinuationStatusV1::Rejected
    )
}

impl World {
    /// Invalidate old-parent responses, continuations, and scheduler entries
    /// exactly once for a strictly advancing reorg epoch. Committed receipts
    /// remain read-only history; only nonterminal old-parent work is fenced.
    pub fn invalidate_cognition_for_reorg(
        &mut self,
        reorg_epoch: u64,
    ) -> Result<JsonValue, WorldError> {
        let mut transaction = self.clone();
        let current_reorg_epoch = transaction
            .cognition
            .get("runtime_binding")
            .and_then(|binding| binding.get("reorg_epoch"))
            .and_then(JsonValue::as_u64)
            .unwrap_or_default();
        let last_invalidation_epoch = transaction
            .cognition
            .get("reorg_invalidation_epoch")
            .and_then(JsonValue::as_u64)
            .unwrap_or(current_reorg_epoch);
        if reorg_epoch <= current_reorg_epoch || reorg_epoch <= last_invalidation_epoch {
            return Err(cognition_validation_error("reorg_epoch_stale"));
        }

        let mut continuations = transaction.cognition_continuations_typed()?;
        let mut scheduler = transaction.cognition_scheduler()?;
        let mut transitioned = Vec::new();
        let mut invalidated = 0u64;
        let mut aborted_commit_events = Vec::new();
        let mut aborted_commit_ids = Vec::new();
        let mut invalidated_response_records = Vec::new();
        let mut response_invalidation_events = Vec::new();

        if let Some(records) = transaction
            .cognition
            .get_mut("commit_records")
            .and_then(JsonValue::as_array_mut)
        {
            for record in records {
                if record.get("status").and_then(JsonValue::as_str) != Some("prepared")
                    || record
                        .get("reorg_epoch")
                        .and_then(JsonValue::as_u64)
                        .is_some_and(|epoch| epoch >= reorg_epoch)
                {
                    continue;
                }
                record["status"] = json!("aborted");
                record["abort_reason"] = json!("reorg_invalidated");
                if let Some(commit_id) = record.get("commit_id").and_then(JsonValue::as_str) {
                    aborted_commit_ids.push(commit_id.to_string());
                }
                if let Some(envelope_digest) =
                    record.get("envelope_digest").and_then(JsonValue::as_str)
                {
                    invalidated_response_records.push((
                        envelope_digest.to_string(),
                        json!({
                            "envelope_idempotency_key": record["envelope_idempotency_key"],
                            "envelope_digest": envelope_digest,
                            "receipt_id": record["receipt_id"],
                            "agent_id": record["agent_id"],
                            "agent_session_id": record["agent_session_id"],
                            "agent_turn_id": record["agent_turn_id"],
                            "decision_request_id": record["decision_request_id"],
                            "request_digest": record["request_digest"],
                            "feedback_id": record["feedback_id"],
                        }),
                    ));
                }
                aborted_commit_events.push(json!({
                    "status": "rejected",
                    "reject_reason": "reorg_invalidated",
                    "envelope_idempotency_key": record["envelope_idempotency_key"],
                    "envelope_digest": record["envelope_digest"],
                    "receipt_id": record["receipt_id"],
                    "agent_id": record["agent_id"],
                    "agent_session_id": record["agent_session_id"],
                    "agent_turn_id": record["agent_turn_id"],
                    "decision_request_id": record["decision_request_id"],
                    "request_digest": record["request_digest"],
                    "feedback_id": record["feedback_id"],
                }));
            }
        }
        if !aborted_commit_events.is_empty() {
            if let Some(index) = transaction
                .cognition
                .get_mut("idempotency_index")
                .and_then(JsonValue::as_object_mut)
            {
                for event in &aborted_commit_events {
                    if let Some(key) = event
                        .get("envelope_idempotency_key")
                        .and_then(JsonValue::as_str)
                    {
                        index
                            .entry(key.to_string())
                            .and_modify(|entry| entry["disposition"] = json!("aborted"));
                    }
                }
            }
            if let Some(staged_actions) = transaction
                .cognition
                .get_mut("staged_actions")
                .and_then(JsonValue::as_object_mut)
            {
                for commit_id in &aborted_commit_ids {
                    staged_actions.remove(commit_id);
                }
            }
            for event in aborted_commit_events {
                append_cognition_event(
                    &mut transaction.cognition,
                    "CognitionTurnCancelled",
                    event.clone(),
                )?;
                append_cognition_event(
                    &mut transaction.cognition,
                    "CognitionTurnCompleted",
                    event,
                )?;
            }
        }

        if let Some(responses) = transaction
            .cognition
            .get_mut("responses")
            .and_then(JsonValue::as_array_mut)
        {
            for response in responses {
                let Some(envelope_digest) = response
                    .get("envelope_digest")
                    .and_then(JsonValue::as_str)
                    .map(str::to_string)
                else {
                    continue;
                };
                if let Some((_, identity)) = invalidated_response_records
                    .iter()
                    .find(|(candidate, _)| candidate == &envelope_digest)
                {
                    response["status"] = json!("invalidated");
                    response["invalidation_reason"] = json!("reorg_invalidated");
                    let mut event = identity.clone();
                    if let Some(response_digest) = response.get("response_digest") {
                        event["response_digest"] = response_digest.clone();
                    }
                    event["status"] = json!("invalidated");
                    event["invalidation_reason"] = json!("reorg_invalidated");
                    response_invalidation_events.push(event);
                }
            }
        }
        for event in response_invalidation_events {
            append_cognition_event(
                &mut transaction.cognition,
                "CognitionResponseInvalidated",
                event,
            )?;
        }

        for continuation in &mut continuations {
            if terminal(continuation.status) || continuation.reorg_epoch >= reorg_epoch {
                continue;
            }
            ContinuationTransition::invalidate_for_reorg_at_tick(
                continuation,
                reorg_epoch,
                transaction.state.time,
            )
            .map_err(|error| cognition_validation_error(error.code()))?;
            invalidated = invalidated.saturating_add(1);
            let deactivated = scheduler
                .deactivate_wake(continuation.wake_id.as_str())
                .map_err(|error| scheduler_error(error.code()))?;
            transitioned.push((continuation.clone(), deactivated));
        }

        // A malformed/old scheduler entry can outlive its typed continuation
        // projection. Fence every stale epoch in all scheduler buckets too.
        let scheduler_snapshot = scheduler.snapshot_json();
        let mut stale_wake_ids = Vec::new();
        if let Some(active) = scheduler_snapshot
            .get("active")
            .and_then(JsonValue::as_array)
        {
            stale_wake_ids.extend(active.iter().filter_map(|wake| {
                (wake
                    .get("reorg_epoch")
                    .and_then(JsonValue::as_u64)
                    .is_some_and(|epoch| epoch < reorg_epoch))
                .then(|| wake.get("wake_id").and_then(JsonValue::as_str))
                .flatten()
                .map(str::to_string)
            }));
        }
        for bucket in ["backpressure", "in_flight"] {
            if let Some(entries) = scheduler_snapshot
                .get(bucket)
                .and_then(JsonValue::as_object)
            {
                stale_wake_ids.extend(entries.values().filter_map(|wake| {
                    (wake
                        .get("reorg_epoch")
                        .and_then(JsonValue::as_u64)
                        .is_some_and(|epoch| epoch < reorg_epoch))
                    .then(|| wake.get("wake_id").and_then(JsonValue::as_str))
                    .flatten()
                    .map(str::to_string)
                }));
            }
        }
        for wake_id in stale_wake_ids {
            scheduler
                .deactivate_wake(&wake_id)
                .map_err(|error| scheduler_error(error.code()))?;
        }

        for (continuation, deactivated) in &transitioned {
            transaction.cognition_commit_continuation_lifecycle_transaction(
                &continuations,
                &scheduler,
                "ContinuationInvalidated",
                continuation,
                deactivated.as_ref(),
            )?;
        }
        transaction.cognition["reorg_invalidation_epoch"] = json!(reorg_epoch);
        if transitioned.is_empty() {
            transaction.cognition_commit_scheduler_transaction(
                &scheduler,
                "ContinuationReorgChecked",
                None,
            )?;
        }
        transaction.persist_runtime_transaction_if_configured()?;
        *self = transaction;
        let report = ContinuationReorgReport {
            terminal_disposition: if invalidated > 0 {
                "reorg_invalidated".to_string()
            } else {
                "already_terminal".to_string()
            },
            provider_invocation_count: 0,
            effect_count: 0,
            receipt_count: 0,
        };
        serde_json::to_value(report).map_err(WorldError::from)
    }
}
