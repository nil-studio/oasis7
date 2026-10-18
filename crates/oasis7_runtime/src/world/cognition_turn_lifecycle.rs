//! Durable pre-response turn lifecycle events.
//!
//! Provider and persistence faults are terminal runtime outcomes, not
//! transport errors that may be inferred from an absent response. These
//! helpers make the design's explicit `ContextCaptured -> RequestDispatched
//! -> CognitionTurnFailed -> CognitionTurnCompleted` branch available to the
//! actor adapter while keeping provider I/O outside `World`.

use super::World;
use crate::error::WorldError;
use serde_json::{Value as JsonValue, json};

fn lifecycle_error(code: &str) -> WorldError {
    super::cognition_validation_error(code)
}

fn bounded(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256
}

fn same_identity(
    event: &JsonValue,
    agent_id: &str,
    agent_session_id: &str,
    agent_turn_id: &str,
    decision_request_id: &str,
    request_digest: &str,
) -> bool {
    event.get("agent_id").and_then(JsonValue::as_str) == Some(agent_id)
        && event.get("agent_session_id").and_then(JsonValue::as_str) == Some(agent_session_id)
        && event.get("agent_turn_id").and_then(JsonValue::as_str) == Some(agent_turn_id)
        && event.get("decision_request_id").and_then(JsonValue::as_str) == Some(decision_request_id)
        && event.get("request_digest").and_then(JsonValue::as_str) == Some(request_digest)
}

fn events_for(world: &World) -> Vec<JsonValue> {
    world
        .cognition
        .get("cognition_journal")
        .and_then(|journal| journal.get("events"))
        .and_then(JsonValue::as_array)
        .cloned()
        .unwrap_or_default()
}

fn validate_identity(
    world: &World,
    agent_id: &str,
    agent_session_id: &str,
    agent_turn_id: &str,
    decision_request_id: &str,
    request_digest: &str,
) -> Result<(), WorldError> {
    if world.cognition_runtime_is_unbound()
        || !bounded(agent_id)
        || !bounded(agent_session_id)
        || !bounded(agent_turn_id)
        || !bounded(decision_request_id)
        || !bounded(request_digest)
    {
        return Err(lifecycle_error("cognition_turn_invalid"));
    }
    if !world.state.agents.is_empty() && !world.state.agents.contains_key(agent_id) {
        return Err(lifecycle_error("cognition_agent_missing"));
    }
    if !world.cognition_turn_is_registered(
        agent_id,
        agent_session_id,
        agent_turn_id,
        decision_request_id,
        request_digest,
    ) {
        return Err(lifecycle_error("cognition_turn_unregistered"));
    }
    Ok(())
}

fn ensure_not_terminal(
    world: &World,
    agent_id: &str,
    agent_session_id: &str,
    agent_turn_id: &str,
    decision_request_id: &str,
    request_digest: &str,
) -> Result<Vec<JsonValue>, WorldError> {
    let events = events_for(world);
    if events.iter().any(|event| {
        same_identity(
            event,
            agent_id,
            agent_session_id,
            agent_turn_id,
            decision_request_id,
            request_digest,
        ) && matches!(
            event.get("event_kind").and_then(JsonValue::as_str),
            Some("CognitionTurnFailed")
                | Some("CognitionTurnCancelled")
                | Some("CognitionTurnCompleted")
        )
    }) {
        return Err(lifecycle_error("cognition_turn_terminal"));
    }
    Ok(events)
}

fn append_turn_event(world: &mut World, kind: &str, details: JsonValue) -> Result<(), WorldError> {
    let mut transaction = world.clone();
    super::super::cognition_persistence_validation::append_cognition_event(
        &mut transaction.cognition,
        kind,
        details,
    )?;
    transaction.persist_runtime_transaction_if_configured()?;
    *world = transaction;
    Ok(())
}

impl World {
    /// Read-only proof that the original context is dispatched and not terminal.
    pub fn validate_active_cognition_context(
        &self,
        identity: (&str, &str, &str, &str, &str),
        context: &str,
        invocation: &str,
    ) -> Result<(), WorldError> {
        let (agent, session, turn, decision, request) = identity;
        let events = ensure_not_terminal(self, agent, session, turn, decision, request)?;
        let matching =
            |event: &JsonValue| same_identity(event, agent, session, turn, decision, request);
        if !events.iter().any(|event| {
            matching(event)
                && event.get("event_kind").and_then(JsonValue::as_str) == Some("ContextCaptured")
                && event.get("context_digest").and_then(JsonValue::as_str) == Some(context)
        }) || !events.iter().any(|event| {
            matching(event)
                && event.get("event_kind").and_then(JsonValue::as_str) == Some("RequestDispatched")
                && event
                    .get("provider_invocation_key")
                    .and_then(JsonValue::as_str)
                    == Some(invocation)
        }) {
            return Err(lifecycle_error("cognition_context_missing"));
        }
        Ok(())
    }

    /// Persist the canonical context capture that precedes provider I/O.
    /// Repeating the exact capture is idempotent; changing its digest is a
    /// request identity conflict.
    pub fn capture_cognition_context(
        &mut self,
        agent_id: &str,
        agent_session_id: &str,
        agent_turn_id: &str,
        decision_request_id: &str,
        request_digest: &str,
        context_digest: &str,
    ) -> Result<(), WorldError> {
        validate_identity(
            self,
            agent_id,
            agent_session_id,
            agent_turn_id,
            decision_request_id,
            request_digest,
        )?;
        if !bounded(context_digest) {
            return Err(lifecycle_error("cognition_context_invalid"));
        }
        let events = ensure_not_terminal(
            self,
            agent_id,
            agent_session_id,
            agent_turn_id,
            decision_request_id,
            request_digest,
        )?;
        if let Some(existing) = events.iter().find(|event| {
            same_identity(
                event,
                agent_id,
                agent_session_id,
                agent_turn_id,
                decision_request_id,
                request_digest,
            ) && event.get("event_kind").and_then(JsonValue::as_str) == Some("ContextCaptured")
        }) {
            if existing.get("context_digest").and_then(JsonValue::as_str) == Some(context_digest) {
                return Ok(());
            }
            return Err(lifecycle_error("cognition_context_conflict"));
        }
        append_turn_event(
            self,
            "ContextCaptured",
            json!({
                "agent_id": agent_id,
                "agent_session_id": agent_session_id,
                "agent_turn_id": agent_turn_id,
                "decision_request_id": decision_request_id,
                "request_digest": request_digest,
                "context_digest": context_digest,
                "logical_tick": self.state.time,
                "status": "running",
            }),
        )
    }

    /// Cancel an unfinished context only inside an authenticated task-control candidate.
    /// Completed turn receipts are immutable; a prepared marker is aborted through
    /// the existing recovery boundary before its context can be replaced.
    pub fn retire_cognition_context_for_task(
        &mut self,
        identity: (&str, &str, &str, &str, &str),
    ) -> Result<(), WorldError> {
        let (agent, session, turn, decision, request) = identity;
        let events = events_for(self);
        let matching =
            |event: &JsonValue| same_identity(event, agent, session, turn, decision, request);
        if events.iter().any(|e| {
            matching(e)
                && matches!(
                    e.get("event_kind").and_then(JsonValue::as_str),
                    Some(
                        "CognitionTurnFailed" | "CognitionTurnCancelled" | "CognitionTurnCompleted"
                    )
                )
        }) {
            return Ok(());
        }
        if !events.iter().any(|e| {
            matching(e)
                && e.get("event_kind").and_then(JsonValue::as_str) == Some("ContextCaptured")
        }) {
            return Err(lifecycle_error("cognition_context_missing"));
        }
        let prepared = self
            .cognition
            .get("commit_records")
            .and_then(JsonValue::as_array)
            .and_then(|records| {
                records.iter().find(|record| {
                    record.get("status").and_then(JsonValue::as_str) == Some("prepared")
                        && record.get("agent_id").and_then(JsonValue::as_str) == Some(agent)
                        && record.get("agent_session_id").and_then(JsonValue::as_str)
                            == Some(session)
                        && record.get("agent_turn_id").and_then(JsonValue::as_str) == Some(turn)
                        && record
                            .get("decision_request_id")
                            .and_then(JsonValue::as_str)
                            == Some(decision)
                        && record.get("request_digest").and_then(JsonValue::as_str) == Some(request)
                })
            })
            .and_then(|record| record.get("commit_id"))
            .and_then(JsonValue::as_str)
            .map(str::to_owned);
        if let Some(marker) = prepared {
            self.abort_cognition_commit(&marker, "cancelled")?;
            return Ok(());
        }
        let mut transaction = self.clone();
        let details = json!({"agent_id": agent, "agent_session_id": session, "agent_turn_id": turn,
            "decision_request_id": decision, "request_digest": request,
            "status": "rejected", "reject_reason": "task_control_changed", "logical_tick": self.state.time});
        super::super::cognition_persistence_validation::append_cognition_event(
            &mut transaction.cognition,
            "CognitionTurnCancelled",
            details.clone(),
        )?;
        super::super::cognition_persistence_validation::append_cognition_event(
            &mut transaction.cognition,
            "CognitionTurnCompleted",
            details,
        )?;
        transaction.persist_runtime_transaction_if_configured()?;
        *self = transaction;
        Ok(())
    }

    /// Persist one provider delivery attempt. Transport retries reuse the
    /// request identity and retry sequence while increasing the attempt.
    #[expect(
        clippy::too_many_arguments,
        reason = "Provider dispatch preserves the stable request/retry identity tuple."
    )]
    pub fn dispatch_cognition_request(
        &mut self,
        agent_id: &str,
        agent_session_id: &str,
        agent_turn_id: &str,
        decision_request_id: &str,
        request_digest: &str,
        provider_invocation_key: &str,
        retry_seq: u64,
        transport_attempt: u64,
    ) -> Result<(), WorldError> {
        validate_identity(
            self,
            agent_id,
            agent_session_id,
            agent_turn_id,
            decision_request_id,
            request_digest,
        )?;
        if !bounded(provider_invocation_key) || retry_seq == 0 || transport_attempt == 0 {
            return Err(lifecycle_error("cognition_dispatch_invalid"));
        }
        let events = ensure_not_terminal(
            self,
            agent_id,
            agent_session_id,
            agent_turn_id,
            decision_request_id,
            request_digest,
        )?;
        if !events.iter().any(|event| {
            same_identity(
                event,
                agent_id,
                agent_session_id,
                agent_turn_id,
                decision_request_id,
                request_digest,
            ) && event.get("event_kind").and_then(JsonValue::as_str) == Some("ContextCaptured")
        }) {
            return Err(lifecycle_error("cognition_context_missing"));
        }
        let dispatches = events.iter().filter(|event| {
            same_identity(
                event,
                agent_id,
                agent_session_id,
                agent_turn_id,
                decision_request_id,
                request_digest,
            ) && event.get("event_kind").and_then(JsonValue::as_str) == Some("RequestDispatched")
        });
        let mut highest_attempt = 0;
        let mut exact_attempt = false;
        for existing in dispatches {
            let existing_key = existing
                .get("provider_invocation_key")
                .and_then(JsonValue::as_str);
            let existing_retry_seq = existing.get("retry_seq").and_then(JsonValue::as_u64);
            let existing_attempt = existing
                .get("transport_attempt")
                .and_then(JsonValue::as_u64);
            if existing_key == Some(provider_invocation_key)
                && existing_retry_seq == Some(retry_seq)
                && existing_attempt == Some(transport_attempt)
            {
                exact_attempt = true;
            }
            if existing_key != Some(provider_invocation_key)
                || existing_retry_seq != Some(retry_seq)
            {
                return Err(lifecycle_error("cognition_dispatch_conflict"));
            }
            highest_attempt = highest_attempt.max(existing_attempt.unwrap_or_default());
        }
        if exact_attempt {
            // An adapter may retry after it has lost the Runtime response;
            // replaying the latest exact tuple is a no-op. A replay of an
            // older attempt after a newer one is stale and must not pretend
            // that transport attempts are monotonic.
            return if transport_attempt == highest_attempt {
                Ok(())
            } else {
                Err(lifecycle_error("cognition_dispatch_conflict"))
            };
        }
        if transport_attempt != highest_attempt.saturating_add(1) {
            return Err(lifecycle_error("cognition_dispatch_conflict"));
        }
        append_turn_event(
            self,
            "RequestDispatched",
            json!({
                "agent_id": agent_id,
                "agent_session_id": agent_session_id,
                "agent_turn_id": agent_turn_id,
                "decision_request_id": decision_request_id,
                "request_digest": request_digest,
                "provider_invocation_key": provider_invocation_key,
                "retry_seq": retry_seq,
                "transport_attempt": transport_attempt,
                "logical_tick": self.state.time,
                "status": "waiting_provider",
            }),
        )
    }

    /// Close a turn with a durable provider/persistence failure before any
    /// response or envelope exists. No receipt, commit marker, world effect,
    /// or implicit retry is created by this method.
    #[expect(
        clippy::too_many_arguments,
        reason = "Turn failure preserves the stable request/retry identity tuple."
    )]
    pub fn fail_cognition_turn(
        &mut self,
        agent_id: &str,
        agent_session_id: &str,
        agent_turn_id: &str,
        decision_request_id: &str,
        request_digest: &str,
        failure_reason: &str,
        retry_seq: u64,
        transport_attempt: u64,
    ) -> Result<(), WorldError> {
        validate_identity(
            self,
            agent_id,
            agent_session_id,
            agent_turn_id,
            decision_request_id,
            request_digest,
        )?;
        if !matches!(
            failure_reason,
            "provider_failure"
                | "persistence_failure"
                | "cognition_failed"
                | "failed_provider"
                | "failed_persist"
        ) || retry_seq == 0
            || transport_attempt > 0 && transport_attempt > u64::from(u16::MAX)
        {
            return Err(lifecycle_error("cognition_failure_invalid"));
        }
        let events = events_for(self);
        let identity_match = |event: &JsonValue| {
            same_identity(
                event,
                agent_id,
                agent_session_id,
                agent_turn_id,
                decision_request_id,
                request_digest,
            )
        };
        let has_context = events.iter().any(|event| {
            identity_match(event)
                && event.get("event_kind").and_then(JsonValue::as_str) == Some("ContextCaptured")
        });
        if !has_context {
            return Err(lifecycle_error("cognition_context_missing"));
        }
        if events.iter().any(|event| {
            identity_match(event)
                && matches!(
                    event.get("event_kind").and_then(JsonValue::as_str),
                    Some("ResponseRecorded")
                        | Some("DecisionEnvelopeSubmitted")
                        | Some("DecisionValidated")
                        | Some("WorldReceiptLinked")
                )
        }) {
            return Err(lifecycle_error("cognition_failure_after_response"));
        }
        if let Some(completed) = events.iter().find(|event| {
            identity_match(event)
                && event.get("event_kind").and_then(JsonValue::as_str)
                    == Some("CognitionTurnCompleted")
                && event.get("status").and_then(JsonValue::as_str) == Some("failed")
        }) {
            if completed.get("failure_reason").and_then(JsonValue::as_str) == Some(failure_reason)
                && completed.get("retry_seq").and_then(JsonValue::as_u64) == Some(retry_seq)
                && completed
                    .get("transport_attempt")
                    .and_then(JsonValue::as_u64)
                    == Some(transport_attempt)
            {
                return Ok(());
            }
            return Err(lifecycle_error("cognition_failure_conflict"));
        }
        let details = || {
            json!({
                "agent_id": agent_id,
                "agent_session_id": agent_session_id,
                "agent_turn_id": agent_turn_id,
                "decision_request_id": decision_request_id,
                "request_digest": request_digest,
                "failure_reason": failure_reason,
                "retry_seq": retry_seq,
                "transport_attempt": transport_attempt,
                "logical_tick": self.state.time,
                "status": "failed",
            })
        };
        let mut transaction = self.clone();
        super::super::cognition_persistence_validation::append_cognition_event(
            &mut transaction.cognition,
            "CognitionTurnFailed",
            details(),
        )?;
        super::super::cognition_persistence_validation::append_cognition_event(
            &mut transaction.cognition,
            "CognitionTurnCompleted",
            details(),
        )?;
        transaction.persist_runtime_transaction_if_configured()?;
        *self = transaction;
        Ok(())
    }
}
