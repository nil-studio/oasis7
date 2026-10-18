//! Small typed helpers for World cognition projection recovery.
//!
//! Keeping repair and journal canonicalization separate from the transaction
//! methods makes the authority boundary easier to audit without changing the
//! persisted projection shape.

use super::super::cognition_persistence_validation::{
    cognition_error, cognition_event_payload_projection, cognition_validation,
};
use super::World;
use super::{
    CognitionJournalEvent, CognitionJournalProjection, CognitionProjection, CognitionReceiptViewV1,
    IdempotencyProjection, WORLD_COMMIT_SCHEMA, append_cognition_event,
};
use crate::cognition::{
    AgentDecisionEnvelopeV1, finality_binding_digest_v1, finality_binding_is_legal,
    world_state_binding_digest_v1,
};
use crate::cognition_recovery::{
    RuntimeCognitionCommitRequestV1, RuntimeCognitionResponseArtifactV1, RuntimeReceiptLineageV1,
    WorldCommitRecordV1, WorldRootViewV1, cognition_digest_v1,
    default_cognition_persistence_projection, response_artifact_digest,
};
use crate::cognition_scheduler::SchedulerWakeV1;
use crate::error::WorldError;
use serde_json::{Value as JsonValue, json};
use std::collections::BTreeMap;

pub(super) fn receipt_digest_for_marker(marker: &WorldCommitRecordV1) -> String {
    let mut receipt = json!({
        "receipt_id": marker.receipt_id,
        "envelope_digest": marker.envelope_digest,
        "action_id": marker.action_id,
        "world_root": marker.parent_world_hash,
    });
    if !marker.response_artifact_digest.is_empty() {
        receipt["response_artifact_digest"] = json!(marker.response_artifact_digest);
    }
    cognition_digest_v1("oasis7.cognition.receipt.v1", &receipt)
}

pub(super) fn validate_prepare_response_artifact(
    envelope: &AgentDecisionEnvelopeV1,
    artifact: &JsonValue,
) -> Result<RuntimeCognitionResponseArtifactV1, WorldError> {
    let identity: RuntimeCognitionResponseArtifactV1 = serde_json::from_value(artifact.clone())
        .map_err(|_| cognition_validation("response_artifact_identity_invalid"))?;
    identity
        .validate()
        .map_err(|_| cognition_validation("response_artifact_identity_invalid"))?;
    let outer = artifact
        .get("outer_lineage")
        .and_then(JsonValue::as_object)
        .ok_or_else(|| cognition_validation("response_outer_lineage_missing"))?;
    let expected_base_world_hash = if envelope.base_world_hash.starts_with("blake3:") {
        envelope.base_world_hash.clone()
    } else {
        world_state_binding_digest_v1(
            &envelope.world_id,
            &envelope.branch_id,
            envelope.finality_epoch,
            envelope.finality_block_hash.as_deref(),
            &envelope.finality_status,
            envelope.base_tick,
            &envelope.base_world_hash,
            envelope.reorg_epoch,
            &envelope.runtime_manifest_hash,
        )
    };
    let expected_manifest_hash = if envelope.runtime_manifest_hash.starts_with("blake3:") {
        envelope.runtime_manifest_hash.clone()
    } else {
        cognition_digest_v1(
            "oasis7.runtime.manifest.v1",
            &envelope.runtime_manifest_hash,
        )
    };
    let outer_block_hash = outer
        .get("finality_block_hash")
        .and_then(JsonValue::as_str)
        .map(str::to_string);
    if outer.get("agent_id").and_then(JsonValue::as_str) != Some(envelope.agent_id.as_str())
        || outer.get("world_id").and_then(JsonValue::as_str) != Some(envelope.world_id.as_str())
        || outer.get("branch_id").and_then(JsonValue::as_str) != Some(envelope.branch_id.as_str())
        || outer.get("finality_epoch").and_then(JsonValue::as_u64) != Some(envelope.finality_epoch)
        || outer_block_hash != envelope.finality_block_hash
        || outer.get("finality_status").and_then(JsonValue::as_str)
            != Some(envelope.finality_status.as_str())
        || outer.get("base_tick").and_then(JsonValue::as_u64) != Some(envelope.base_tick)
        || outer.get("base_world_hash").and_then(JsonValue::as_str)
            != Some(expected_base_world_hash.as_str())
        || outer.get("reorg_epoch").and_then(JsonValue::as_u64) != Some(envelope.reorg_epoch)
        || outer
            .get("runtime_manifest_hash")
            .and_then(JsonValue::as_str)
            != Some(expected_manifest_hash.as_str())
    {
        return Err(cognition_validation("response_outer_lineage_mismatch"));
    }
    if identity.agent_session_id != envelope.agent_session_id
        || identity.agent_turn_id != envelope.agent_turn_id
        || identity.decision_request_id != envelope.decision_request_id
        || identity.request_digest != envelope.request_digest
    {
        return Err(cognition_validation("response_artifact_lineage_mismatch"));
    }
    Ok(identity)
}

fn response_artifact_with_outer_lineage(
    artifact: &RuntimeCognitionResponseArtifactV1,
    request: &RuntimeCognitionCommitRequestV1,
) -> Result<JsonValue, WorldError> {
    let mut value = serde_json::to_value(artifact).map_err(WorldError::from)?;
    value["outer_lineage"] = json!({
        "agent_id": request.agent_id,
        "world_id": request.captured_base_binding.world_id,
        "branch_id": request.captured_base_binding.branch_id,
        "finality_epoch": request.captured_base_binding.finality_epoch,
        "finality_block_hash": request.captured_base_binding.finality_block_hash,
        "finality_status": request.captured_base_binding.finality_status,
        "base_tick": request.captured_base_binding.base_tick,
        "base_world_hash": request.captured_base_binding.base_world_hash,
        "reorg_epoch": request.captured_base_binding.reorg_epoch,
        "runtime_manifest_hash": request.captured_base_binding.runtime_manifest_hash,
    });
    Ok(value)
}

impl World {
    fn bind_cognition_response_artifact(
        &mut self,
        commit_id: &str,
        response_artifact: &RuntimeCognitionResponseArtifactV1,
    ) -> Result<(), WorldError> {
        let mut projection = self.cognition.clone();
        let envelope_digest = projection
            .get("commit_records")
            .and_then(JsonValue::as_array)
            .and_then(|records| {
                records.iter().find(|record| {
                    record.get("commit_id").and_then(JsonValue::as_str) == Some(commit_id)
                })
            })
            .and_then(|record| record.get("envelope_digest"))
            .and_then(JsonValue::as_str)
            .ok_or_else(|| cognition_validation("envelope_digest_missing"))?
            .to_string();
        let response_digest = projection
            .get("responses")
            .and_then(JsonValue::as_array)
            .and_then(|responses| {
                responses.iter().find(|response| {
                    response.get("envelope_digest").and_then(JsonValue::as_str)
                        == Some(envelope_digest.as_str())
                })
            })
            .and_then(|response| response.get("response_artifact"))
            .map(response_artifact_digest)
            .unwrap_or_else(|| {
                response_artifact_digest(
                    &serde_json::to_value(response_artifact).unwrap_or(JsonValue::Null),
                )
            });
        let record = projection
            .get_mut("commit_records")
            .and_then(JsonValue::as_array_mut)
            .and_then(|records| {
                records.iter_mut().find(|record| {
                    record.get("commit_id").and_then(JsonValue::as_str) == Some(commit_id)
                })
            })
            .ok_or_else(|| cognition_validation("commit_record_missing"))?;
        record["response_context_discriminator"] = json!(response_artifact.context_discriminator);
        record["response_context_version"] = json!(response_artifact.context_version);
        record["response_retry_seq"] = json!(response_artifact.retry_seq);
        record["transport_attempt"] = json!(response_artifact.transport_attempt);
        record["response_artifact_digest"] = json!(response_digest);
        let receipt_id = record["receipt_id"]
            .as_str()
            .ok_or_else(|| cognition_validation("receipt_id_missing"))?
            .to_string();
        let envelope_digest = record["envelope_digest"]
            .as_str()
            .ok_or_else(|| cognition_validation("envelope_digest_missing"))?
            .to_string();
        let action_id = record["action_id"]
            .as_str()
            .ok_or_else(|| cognition_validation("action_id_missing"))?
            .to_string();
        let world_root = record["parent_world_hash"]
            .as_str()
            .ok_or_else(|| cognition_validation("parent_world_hash_missing"))?
            .to_string();
        record["receipt_digest"] = json!(cognition_digest_v1(
            "oasis7.cognition.receipt.v1",
            &json!({
                "receipt_id": receipt_id,
                "envelope_digest": envelope_digest,
                "action_id": action_id,
                "world_root": world_root,
                "response_artifact_digest": response_digest,
            })
        ));
        self.cognition = projection;
        Ok(())
    }
}

impl World {
    pub(in crate::world) fn has_recorded_stale_cognition_rejection(
        &self,
        envelope: &AgentDecisionEnvelopeV1,
    ) -> Result<bool, WorldError> {
        let parsed: CognitionProjection = serde_json::from_value(self.cognition.clone())
            .map_err(|error| cognition_error("invalid_cognition_projection", error))?;
        let Some(entry) = parsed
            .idempotency_index
            .get(&envelope.envelope_idempotency_key)
        else {
            return Ok(false);
        };
        if entry.envelope_digest != envelope.envelope_digest || entry.disposition != "rejected" {
            return Ok(false);
        }
        let events = self.cognition["cognition_journal"]["events"]
            .as_array()
            .ok_or_else(|| cognition_validation("cognition_events_not_array"))?;
        let matches_identity = |event: &JsonValue, kind: &str| {
            event["kind"] == kind
                && event["agent_id"] == envelope.agent_id
                && event["agent_session_id"] == envelope.agent_session_id
                && event["agent_turn_id"] == envelope.agent_turn_id
                && event["decision_request_id"] == envelope.decision_request_id
                && event["request_digest"] == envelope.request_digest
                && event["envelope_digest"] == envelope.envelope_digest
                && event["envelope_idempotency_key"] == envelope.envelope_idempotency_key
        };
        Ok(events
            .iter()
            .any(|event| matches_identity(event, "DecisionRejected"))
            && events.iter().any(|event| {
                matches_identity(event, "CognitionTurnCompleted") && event["status"] == "rejected"
            }))
    }

    pub fn commit_cognition_action(
        &mut self,
        request: RuntimeCognitionCommitRequestV1,
        action: crate::Action,
        response_artifact: RuntimeCognitionResponseArtifactV1,
    ) -> Result<(WorldCommitRecordV1, RuntimeReceiptLineageV1), WorldError> {
        self.commit_cognition_action_inner(request, action, response_artifact, None, &[])
    }

    /// Task execution carries public authority separately from private context input.
    pub fn commit_task_cognition_action(
        &mut self,
        request: RuntimeCognitionCommitRequestV1,
        action: crate::Action,
        response_artifact: RuntimeCognitionResponseArtifactV1,
        task_authority: (&oasis7_agent_api::game::GameTaskFenceV1, &str, u64),
        staged: &[crate::service_admission::CanonicalIntentResultV1],
    ) -> Result<(WorldCommitRecordV1, RuntimeReceiptLineageV1), WorldError> {
        self.commit_cognition_action_inner(
            request,
            action,
            response_artifact,
            Some(task_authority),
            staged,
        )
    }

    fn commit_cognition_action_inner(
        &mut self,
        request: RuntimeCognitionCommitRequestV1,
        action: crate::Action,
        response_artifact: RuntimeCognitionResponseArtifactV1,
        task_authority: Option<(&oasis7_agent_api::game::GameTaskFenceV1, &str, u64)>,
        staged: &[crate::service_admission::CanonicalIntentResultV1],
    ) -> Result<(WorldCommitRecordV1, RuntimeReceiptLineageV1), WorldError> {
        request
            .validate()
            .map_err(|error| cognition_validation(error.code()))?;
        response_artifact
            .validate_for_request(&request)
            .map_err(|error| cognition_validation(error.code()))?;
        let action_value = serde_json::to_value(&action).map_err(WorldError::from)?;
        let response_artifact_value =
            response_artifact_with_outer_lineage(&response_artifact, &request)?;
        let action_digest = cognition_digest_v1("oasis7.cognition.action.v1", &action_value);

        // Idempotency is keyed by the complete outer request identity, not by
        // the current World head. A retry can arrive after the committed
        // action advanced that head; look up the durable marker before doing
        // the optimistic-concurrency check so an exact replay returns the
        // original receipt without creating another terminal event/effect.
        if let Ok(parsed) =
            serde_json::from_value::<CognitionProjection>(if self.cognition.is_null() {
                default_cognition_persistence_projection()
            } else {
                self.cognition.clone()
            })
            && let Some(existing) = parsed.commit_records.iter().find(|record| {
                record.agent_id == request.agent_id
                    && record.agent_session_id == request.agent_session_id
                    && record.agent_turn_id == request.agent_turn_id
                    && record.decision_request_id == request.decision_request_id
                    && record.request_digest == request.request_digest
            })
        {
            let stored_action_digest = self
                .cognition
                .get("action_digests")
                .and_then(JsonValue::as_object)
                .and_then(|digests| digests.get(&existing.commit_id))
                .and_then(JsonValue::as_str)
                .map(str::to_string)
                .or_else(|| {
                    parsed
                        .staged_actions
                        .get(&existing.commit_id)
                        .map(|staged| cognition_digest_v1("oasis7.cognition.action.v1", staged))
                });
            if existing.status == "prepared" && stored_action_digest.is_none() {
                return Err(cognition_validation("cognition_staged_action_missing"));
            }
            if stored_action_digest
                .as_deref()
                .is_some_and(|stored| stored != action_digest)
            {
                return Err(cognition_validation("envelope_idempotency_conflict"));
            }
            if existing.status == "aborted" {
                return Err(cognition_validation(
                    existing
                        .abort_reason
                        .as_deref()
                        .unwrap_or("cognition_commit_aborted"),
                ));
            }
            let response_matches = parsed.responses.iter().any(|response| {
                response.envelope_digest == existing.envelope_digest
                    && response.response_artifact.as_ref() == Some(&response_artifact_value)
            });
            if !response_matches {
                return Err(cognition_validation("response_artifact_lineage_mismatch"));
            }
            if existing.status == "committed" {
                let lineage = self.read_runtime_receipt_lineage(&existing.receipt_id)?;
                self.verify_runtime_receipt_lineage(&lineage)?;
                return Ok((existing.clone(), lineage));
            }
        }

        crate::service_admission::game_task::validate_task_cognition_admission(
            self,
            &request,
            task_authority,
            staged,
        )
        .map_err(|_| cognition_validation("cognition_task_fenced"))?;

        let authority = self.current_cognition_runtime_authority()?;
        let base_binding = self.cognition_runtime_base_binding(&authority);
        if request.captured_base_binding != base_binding {
            let (stale_digest, stale_key) =
                Self::stale_cognition_rejection_identity(&request, &action)?;
            let mut transaction = self.clone();
            transaction.record_stale_cognition_rejection(
                &request.agent_id,
                &request.agent_session_id,
                &request.agent_turn_id,
                &request.decision_request_id,
                &request.request_digest,
                &stale_digest,
                &stale_key,
            )?;
            transaction.persist_runtime_transaction_if_configured()?;
            *self = transaction;
            return Err(stale_base_error());
        }
        if !self.state.agents.is_empty() && !self.state.agents.contains_key(&request.agent_id) {
            return Err(cognition_validation("cognition_agent_missing"));
        }
        if action.actor_id() != Some(request.agent_id.as_str()) {
            return Err(cognition_validation("cognition_action_identity_mismatch"));
        }
        let mut transaction = self.clone();
        let envelope = AgentDecisionEnvelopeV1 {
            schema_version: crate::cognition::AGENT_DECISION_ENVELOPE_V1_SCHEMA
                .to_string(),
            world_id: base_binding.world_id.clone(),
            agent_id: request.agent_id,
            branch_id: base_binding.branch_id.clone(),
            finality_epoch: base_binding.finality_epoch,
            finality_block_hash: base_binding.finality_block_hash.clone(),
            finality_status: base_binding.finality_status.clone(),
            agent_session_id: request.agent_session_id,
            agent_turn_id: request.agent_turn_id,
            decision_request_id: request.decision_request_id,
            retry_seq: request.retry_seq,
            base_tick: self.state.time,
            base_world_hash: base_binding.base_world_hash.clone(),
            reorg_epoch: base_binding.reorg_epoch,
            runtime_manifest_hash: base_binding.runtime_manifest_hash.clone(),
            capability_snapshot_hash: request.capability_snapshot_hash,
            authority_context_hash: request.authority_context_hash,
            observation_digest: request.observation_digest,
            context_digest: request.context_digest,
            issued_at_tick: self.state.time,
            valid_until_tick: self.state.time,
            preconditions: Vec::new(),
            decision_kind: "act".to_string(),
            action: action_value,
            request_digest: request.request_digest,
            decision_digest: String::new(),
            envelope_digest: String::new(),
            provider_invocation_key: String::new(),
            envelope_idempotency_key: String::new(),
            origin_intent_ref: None,
            source: "continuous-agent-runtime".to_string(),
        };
        let mut envelope = envelope;
        envelope.decision_digest = envelope.derive_decision_digest();
        envelope.envelope_digest = envelope.derive_envelope_digest();
        envelope.provider_invocation_key = envelope.derive_provider_invocation_key();
        envelope.envelope_idempotency_key = envelope.derive_envelope_idempotency_key();
        let prepared = transaction
            .prepare_cognition_envelope(envelope, Some(response_artifact_value.clone()))?;
        let existing_response = transaction
            .cognition
            .get("responses")
            .and_then(JsonValue::as_array)
            .and_then(|responses| {
                responses.iter().find(|response| {
                    response.get("envelope_digest").and_then(JsonValue::as_str)
                        == Some(prepared.envelope_digest.as_str())
                })
            })
            .and_then(|response| response.get("response_artifact"));
        if existing_response != Some(&response_artifact_value) {
            return Err(cognition_validation("response_artifact_lineage_mismatch"));
        }
        if prepared.status == "committed" {
            let lineage = transaction.read_runtime_receipt_lineage(&prepared.receipt_id)?;
            transaction.verify_runtime_receipt_lineage(&lineage)?;
            *self = transaction;
            return Ok((prepared, lineage));
        }
        transaction.bind_cognition_response_artifact(&prepared.commit_id, &response_artifact)?;
        let committed = match transaction.finalize_cognition_commit(&prepared.commit_id) {
            Ok(committed) => committed,
            Err(error) => {
                if matches!(&error, WorldError::ResourceBalanceInvalid { reason } if reason.starts_with("agent_delegation:"))
                {
                    *self = transaction;
                }
                return Err(error);
            }
        };
        let lineage = transaction.read_runtime_receipt_lineage(&committed.receipt_id)?;
        transaction.verify_runtime_receipt_lineage(&lineage)?;
        *self = transaction;
        Ok((committed, lineage))
    }

    pub(in crate::world) fn validate_persisted_cognition_wakes(
        &self,
        scheduler_state: &JsonValue,
    ) -> Result<(), WorldError> {
        let scheduler_state = self.hydrate_legacy_wake_request_digests(scheduler_state)?;
        let mut values = Vec::new();
        if let Some(active) = scheduler_state.get("active").and_then(JsonValue::as_array) {
            values.extend(active.iter().cloned());
        }
        for bucket in ["backpressure", "in_flight"] {
            if let Some(entries) = scheduler_state.get(bucket).and_then(JsonValue::as_object) {
                values.extend(entries.values().cloned());
            }
        }
        for value in values {
            let wake: SchedulerWakeV1 = serde_json::from_value(value)
                .map_err(|error| cognition_error("invalid_scheduler_wake", error))?;
            self.validate_cognition_wake_binding(&wake)?;
        }
        Ok(())
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Stale rejection persists the complete request identity and envelope binding explicitly."
    )]
    pub(in crate::world) fn record_stale_cognition_rejection(
        &mut self,
        agent_id: &str,
        agent_session_id: &str,
        agent_turn_id: &str,
        decision_request_id: &str,
        request_digest: &str,
        envelope_digest: &str,
        envelope_idempotency_key: &str,
    ) -> Result<(), WorldError> {
        let mut next = if self.cognition.is_null() {
            default_cognition_persistence_projection()
        } else {
            self.cognition.clone()
        };
        let parsed: CognitionProjection = serde_json::from_value(next.clone())
            .map_err(|error| cognition_error("invalid_cognition_projection", error))?;
        let existing = parsed.idempotency_index.get(envelope_idempotency_key);
        if existing.is_some_and(|entry| entry.envelope_digest != envelope_digest) {
            return Err(cognition_validation("envelope_idempotency_conflict"));
        }
        if existing.is_some_and(|entry| entry.disposition != "rejected") {
            return Err(cognition_validation("envelope_idempotency_conflict"));
        }
        let events = next["cognition_journal"]["events"]
            .as_array()
            .ok_or_else(|| cognition_validation("cognition_events_not_array"))?;
        let matches_identity = |event: &JsonValue, kind: &str| {
            event["kind"] == kind
                && event["agent_id"] == agent_id
                && event["agent_session_id"] == agent_session_id
                && event["agent_turn_id"] == agent_turn_id
                && event["decision_request_id"] == decision_request_id
                && event["request_digest"] == request_digest
                && event["envelope_digest"] == envelope_digest
                && event["envelope_idempotency_key"] == envelope_idempotency_key
        };
        let rejected = events
            .iter()
            .any(|event| matches_identity(event, "DecisionRejected"));
        let completed = events.iter().any(|event| {
            matches_identity(event, "CognitionTurnCompleted") && event["status"] == "rejected"
        });
        if existing.is_some_and(|entry| entry.disposition == "rejected") {
            return if rejected && completed {
                Ok(())
            } else {
                Err(cognition_validation(
                    "cognition_rejection_projection_mismatch",
                ))
            };
        }
        if rejected || completed {
            return Err(cognition_validation(
                "cognition_rejection_projection_mismatch",
            ));
        }
        let object = next
            .as_object_mut()
            .ok_or_else(|| cognition_validation("cognition_projection_not_object"))?;
        object
            .entry("idempotency_index")
            .or_insert_with(|| JsonValue::Object(serde_json::Map::new()))
            .as_object_mut()
            .ok_or_else(|| cognition_validation("idempotency_index_not_object"))?
            .insert(
                envelope_idempotency_key.to_string(),
                json!({
                    "envelope_digest": envelope_digest,
                    "disposition": "rejected",
                    "receipt_id": ""
                }),
            );
        let identity = json!({
            "agent_id": agent_id,
            "agent_session_id": agent_session_id,
            "agent_turn_id": agent_turn_id,
            "decision_request_id": decision_request_id,
            "request_digest": request_digest,
            "envelope_digest": envelope_digest,
            "envelope_idempotency_key": envelope_idempotency_key,
        });
        let mut rejection = identity.clone();
        rejection["reject_reason"] = json!("stale_base");
        append_cognition_event(&mut next, "DecisionRejected", rejection)?;
        let mut completion = identity;
        completion["status"] = json!("rejected");
        append_cognition_event(&mut next, "CognitionTurnCompleted", completion)?;
        let recovery = next
            .as_object_mut()
            .and_then(|object| object.get_mut("recovery"))
            .and_then(JsonValue::as_object_mut)
            .ok_or_else(|| cognition_validation("recovery_projection_not_object"))?;
        recovery.insert("disposition".to_string(), json!("rejected"));
        recovery.insert("reject_reason".to_string(), json!("stale_base"));
        recovery.insert(
            "idempotency_key".to_string(),
            json!(envelope_idempotency_key),
        );
        self.cognition = next;
        Ok(())
    }
}

pub(super) fn reconcile_committed_projection(
    projection: &mut JsonValue,
) -> Result<(Vec<String>, u64), WorldError> {
    let mut parsed: CognitionProjection = serde_json::from_value(projection.clone())
        .map_err(|error| cognition_error("invalid_cognition_projection", error))?;
    let markers: Vec<WorldCommitRecordV1> = parsed
        .commit_records
        .iter()
        .filter(|marker| marker.status == "committed")
        .cloned()
        .collect();
    let mut repaired_ids = Vec::new();
    let mut repairs = 0u64;
    let mut journal_repaired = false;

    for marker in markers {
        let receipt_present = parsed.receipt_registry.iter().any(|receipt| {
            receipt.receipt_id == marker.receipt_id
                && receipt.receipt_digest == marker.receipt_digest
        });
        if !receipt_present {
            parsed.receipt_registry.push(CognitionReceiptViewV1 {
                receipt_id: marker.receipt_id.clone(),
                receipt_digest: marker.receipt_digest.clone(),
            });
            record_repair(&mut repaired_ids, "receipt_registry");
            repairs = repairs.saturating_add(1);
        }

        let idempotency_present = parsed
            .idempotency_index
            .get(&marker.envelope_idempotency_key)
            .is_some_and(|entry| {
                entry.envelope_digest == marker.envelope_digest
                    && entry.disposition == "committed"
                    && entry.receipt_id == marker.receipt_id
            });
        if !idempotency_present {
            parsed.idempotency_index.insert(
                marker.envelope_idempotency_key.clone(),
                IdempotencyProjection {
                    envelope_digest: marker.envelope_digest.clone(),
                    disposition: "committed".to_string(),
                    receipt_id: marker.receipt_id.clone(),
                },
            );
            record_repair(&mut repaired_ids, "idempotency_index");
            repairs = repairs.saturating_add(1);
        }

        let linked_present = parsed.cognition_journal.events.iter().any(|event| {
            event.kind == "WorldReceiptLinked"
                && event.envelope_idempotency_key == marker.envelope_idempotency_key
                && (event.receipt_id.as_deref() == Some(marker.receipt_id.as_str())
                    // Sparse markers are the backwards-compatible projection
                    // shape used by pre-lineage snapshots. Their link event
                    // predates receipt identity being persisted, and the
                    // validator intentionally treats that absent field as
                    // compatible. Do not synthesize a dense event during a
                    // read/repair merely because the legacy event is sparse.
                    || (event.receipt_id.is_none() && marker.agent_id.is_empty()))
        });
        if !linked_present {
            let next_seq = parsed
                .cognition_journal
                .events
                .iter()
                .map(|event| event.journal_seq)
                .max()
                .unwrap_or_default()
                .saturating_add(1);
            parsed.cognition_journal.events.push(CognitionJournalEvent {
                journal_seq: next_seq,
                kind: "WorldReceiptLinked".to_string(),
                envelope_idempotency_key: marker.envelope_idempotency_key.clone(),
                receipt_id: Some(marker.receipt_id.clone()),
                agent_id: (!marker.agent_id.is_empty()).then(|| marker.agent_id.clone()),
                agent_session_id: (!marker.agent_session_id.is_empty())
                    .then(|| marker.agent_session_id.clone()),
                agent_turn_id: (!marker.agent_turn_id.is_empty())
                    .then(|| marker.agent_turn_id.clone()),
                decision_request_id: (!marker.decision_request_id.is_empty())
                    .then(|| marker.decision_request_id.clone()),
                request_digest: (!marker.request_digest.is_empty())
                    .then(|| marker.request_digest.clone()),
                feedback_id: (!marker.feedback_id.is_empty()).then(|| marker.feedback_id.clone()),
                ..CognitionJournalEvent::default()
            });
            parsed.cognition_journal.head_seq = next_seq;
            record_repair(&mut repaired_ids, "world_receipt_link");
            repairs = repairs.saturating_add(1);
            journal_repaired = true;
        }

        let completed_present = parsed.cognition_journal.events.iter().any(|event| {
            event.kind == "CognitionTurnCompleted"
                && event.envelope_idempotency_key == marker.envelope_idempotency_key
                && (event.receipt_id.as_deref() == Some(marker.receipt_id.as_str())
                    || (event.receipt_id.is_none() && marker.agent_id.is_empty()))
                && event.status == "committed"
        });
        if !completed_present {
            let next_seq = parsed
                .cognition_journal
                .events
                .iter()
                .map(|event| event.journal_seq)
                .max()
                .unwrap_or_default()
                .saturating_add(1);
            parsed.cognition_journal.events.push(CognitionJournalEvent {
                journal_seq: next_seq,
                kind: "CognitionTurnCompleted".to_string(),
                envelope_idempotency_key: marker.envelope_idempotency_key.clone(),
                receipt_id: Some(marker.receipt_id.clone()),
                agent_id: (!marker.agent_id.is_empty()).then(|| marker.agent_id.clone()),
                agent_session_id: (!marker.agent_session_id.is_empty())
                    .then(|| marker.agent_session_id.clone()),
                agent_turn_id: (!marker.agent_turn_id.is_empty())
                    .then(|| marker.agent_turn_id.clone()),
                decision_request_id: (!marker.decision_request_id.is_empty())
                    .then(|| marker.decision_request_id.clone()),
                request_digest: (!marker.request_digest.is_empty())
                    .then(|| marker.request_digest.clone()),
                feedback_id: (!marker.feedback_id.is_empty()).then(|| marker.feedback_id.clone()),
                status: "committed".to_string(),
                ..CognitionJournalEvent::default()
            });
            parsed.cognition_journal.head_seq = next_seq;
            record_repair(&mut repaired_ids, "cognition_turn_completed");
            repairs = repairs.saturating_add(1);
            journal_repaired = true;
        }

        if let Some(derived) = RuntimeReceiptLineageV1::from_durable_commit_record(&marker) {
            if let Some(existing) = parsed
                .receipt_lineage_registry
                .iter()
                .find(|lineage| lineage.receipt_id == marker.receipt_id)
            {
                if existing != &derived {
                    return Err(cognition_validation("receipt_lineage_binding_mismatch"));
                }
            } else {
                parsed.receipt_lineage_registry.push(derived);
                record_repair(&mut repaired_ids, "receipt_lineage_registry");
                repairs = repairs.saturating_add(1);
            }
        }
    }

    if repairs > 0 {
        // Registry/idempotency repairs are projection-only recovery. The
        // journal is the durable event source and must remain byte-for-byte
        // stable unless recovery actually appends a missing lifecycle event.
        if journal_repaired {
            refresh_cognition_journal(&mut parsed.cognition_journal);
            *projection = serde_json::to_value(parsed).map_err(WorldError::from)?;
        } else {
            let object = projection
                .as_object_mut()
                .ok_or_else(|| cognition_validation("cognition_projection_not_object"))?;
            object.insert(
                "receipt_registry".to_string(),
                serde_json::to_value(parsed.receipt_registry).map_err(WorldError::from)?,
            );
            object.insert(
                "idempotency_index".to_string(),
                serde_json::to_value(parsed.idempotency_index).map_err(WorldError::from)?,
            );
            object.insert(
                "receipt_lineage_registry".to_string(),
                serde_json::to_value(parsed.receipt_lineage_registry).map_err(WorldError::from)?,
            );
        }
    }
    Ok((repaired_ids, repairs))
}

pub(super) fn stale_base_error() -> WorldError {
    WorldError::DistributedValidationFailed {
        reason: "stale_base".to_string(),
    }
}

fn refresh_cognition_journal(journal: &mut CognitionJournalProjection) {
    let mut previous_event_digest = String::new();
    for event in &mut journal.events {
        if event.schema_version.is_empty() {
            event.schema_version = "cognition-journal-event.v1".to_string();
        }
        if event.event_kind.is_empty() {
            event.event_kind = event.kind.clone();
        }
        if event.kind.is_empty() {
            event.kind = event.event_kind.clone();
        }
        if event.world_id.is_empty() {
            event.world_id = "unbound".to_string();
        }
        if event.branch_id.is_empty() {
            event.branch_id = "main".to_string();
        }
        if event.finality_status.is_empty() {
            event.finality_status = "pending".to_string();
        }
        if event.status.is_empty() {
            event.status = event
                .extra
                .get("status")
                .and_then(JsonValue::as_str)
                .unwrap_or("pending")
                .to_string();
        }
        if event.payload_digest.is_empty() {
            let payload = cognition_event_payload_projection(
                &serde_json::to_value(&*event).unwrap_or(JsonValue::Null),
            );
            event.payload_digest =
                cognition_digest_v1("oasis7.cognition.event-payload.v1", &payload);
        }
        if event.parent_event_digest.is_empty() && event.journal_seq > 1 {
            event.parent_event_digest = previous_event_digest.clone();
        }
        event.event_digest = None;
        let unsigned = serde_json::to_value(&*event).unwrap_or(JsonValue::Null);
        let digest = cognition_digest_v1("oasis7.cognition.event.v1", &unsigned);
        event.event_digest = Some(digest.clone());
        previous_event_digest = digest;
    }
    journal.head_seq = journal.head_seq.max(
        journal
            .events
            .iter()
            .map(|event| event.journal_seq)
            .max()
            .unwrap_or_default(),
    );
    let events = serde_json::to_value(&journal.events).unwrap_or(JsonValue::Array(Vec::new()));
    journal.head_digest = cognition_digest_v1(
        "oasis7.cognition.journal-head.v1",
        &json!({"head_seq": journal.head_seq, "events": events}),
    );
}

pub(super) fn record_repair(repaired_ids: &mut Vec<String>, id: &str) {
    if !repaired_ids.iter().any(|existing| existing == id) {
        repaired_ids.push(id.to_string());
    }
}

pub(super) fn validate_lineage_binding(
    projection: &CognitionProjection,
    marker: &WorldCommitRecordV1,
    lineage: &RuntimeReceiptLineageV1,
) -> Result<(), WorldError> {
    if marker.receipt_digest != lineage.receipt_digest
        || marker.envelope_digest != lineage.envelope_digest
        || marker.action_id != lineage.action_id
        || !projection.receipt_registry.iter().any(|receipt| {
            receipt.receipt_id == marker.receipt_id
                && receipt.receipt_digest == marker.receipt_digest
        })
        || !projection
            .idempotency_index
            .get(&marker.envelope_idempotency_key)
            .is_some_and(|entry| {
                entry.envelope_digest == marker.envelope_digest
                    && entry.disposition == "committed"
                    && entry.receipt_id == marker.receipt_id
            })
        || !projection.cognition_journal.events.iter().any(|event| {
            event.kind == "WorldReceiptLinked"
                && event.envelope_idempotency_key == marker.envelope_idempotency_key
                && event
                    .receipt_id
                    .as_deref()
                    .is_none_or(|receipt_id| receipt_id == marker.receipt_id)
                && event
                    .agent_id
                    .as_deref()
                    .is_none_or(|agent_id| agent_id == lineage.agent_id)
                && event
                    .agent_session_id
                    .as_deref()
                    .is_none_or(|session_id| session_id == lineage.agent_session_id)
                && event
                    .agent_turn_id
                    .as_deref()
                    .is_none_or(|turn_id| turn_id == lineage.agent_turn_id)
                && event
                    .decision_request_id
                    .as_deref()
                    .is_none_or(|request_id| request_id == lineage.decision_request_id)
                && event
                    .request_digest
                    .as_deref()
                    .is_none_or(|request_digest| request_digest == lineage.request_digest)
                && event
                    .feedback_id
                    .as_deref()
                    .is_none_or(|feedback_id| feedback_id == lineage.feedback_id)
        })
    {
        return Err(cognition_validation("receipt_lineage_binding_mismatch"));
    }
    if !marker.agent_id.is_empty()
        && (marker.agent_id != lineage.agent_id
            || marker.agent_session_id != lineage.agent_session_id
            || marker.agent_turn_id != lineage.agent_turn_id
            || marker.decision_request_id != lineage.decision_request_id
            || marker.request_digest != lineage.request_digest
            || marker.feedback_id != lineage.feedback_id)
    {
        return Err(cognition_validation("receipt_lineage_binding_mismatch"));
    }
    if !marker.agent_id.is_empty()
        && !projection.cognition_journal.events.iter().any(|event| {
            event.kind == "WorldReceiptLinked"
                && event.envelope_idempotency_key == marker.envelope_idempotency_key
                && event.receipt_id.as_deref() == Some(marker.receipt_id.as_str())
                && event.agent_id.as_deref() == Some(marker.agent_id.as_str())
                && event.agent_session_id.as_deref() == Some(marker.agent_session_id.as_str())
                && event.agent_turn_id.as_deref() == Some(marker.agent_turn_id.as_str())
                && event.decision_request_id.as_deref() == Some(marker.decision_request_id.as_str())
                && event.request_digest.as_deref() == Some(marker.request_digest.as_str())
                && event.feedback_id.as_deref() == Some(marker.feedback_id.as_str())
        })
    {
        return Err(cognition_validation("receipt_lineage_binding_mismatch"));
    }
    Ok(())
}

pub(super) fn select_commit_record(
    records: &[WorldCommitRecordV1],
) -> Option<&WorldCommitRecordV1> {
    records
        .iter()
        .max_by_key(|record| (record.cognition_journal_seq, record.commit_id.as_str()))
}

pub(super) fn validate_marker(marker: &WorldCommitRecordV1) -> Result<(), WorldError> {
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
    if marker.schema_version != WORLD_COMMIT_SCHEMA
        || marker.commit_id.trim().is_empty()
        || marker.envelope_idempotency_key.trim().is_empty()
        || marker.envelope_digest.trim().is_empty()
        || marker.world_id.trim().is_empty()
        || marker.branch_id.trim().is_empty()
        || marker
            .finality_block_hash
            .as_deref()
            .is_some_and(|hash| hash.trim().is_empty())
        || marker.finality_status.trim().is_empty()
        || marker.finality_binding_digest.trim().is_empty()
        || marker.runtime_manifest_hash.trim().is_empty()
        || marker.action_id.trim().is_empty()
        || marker.parent_world_hash.trim().is_empty()
        || marker.staged_event_root.trim().is_empty()
        || marker.staged_state_root.trim().is_empty()
        || marker.receipt_id.trim().is_empty()
        || marker.receipt_digest.trim().is_empty()
        || !abort_shape_valid
    {
        return Err(WorldError::DistributedValidationFailed {
            reason: "invalid_commit_record".to_string(),
        });
    }
    if !finality_binding_is_legal(
        &marker.finality_status,
        marker.finality_block_hash.as_deref(),
    ) {
        return Err(WorldError::DistributedValidationFailed {
            reason: "invalid_commit_record_finality".to_string(),
        });
    }
    let dense = [
        &marker.agent_id,
        &marker.agent_session_id,
        &marker.agent_turn_id,
        &marker.decision_request_id,
        &marker.request_digest,
        &marker.feedback_id,
    ];
    if marker.agent_id.is_empty() != dense.iter().all(|value| value.is_empty())
        || (!marker.agent_id.is_empty() && dense.iter().any(|value| value.trim().is_empty()))
    {
        return Err(WorldError::DistributedValidationFailed {
            reason: "invalid_commit_record_identity".to_string(),
        });
    }
    if !marker.agent_id.is_empty() {
        let expected_commit_id = cognition_digest_v1(
            "oasis7.cognition.commit-id.v1",
            &json!({
                "envelope_digest": marker.envelope_digest,
                "envelope_idempotency_key": marker.envelope_idempotency_key,
            }),
        );
        let expected_receipt_id = cognition_digest_v1(
            "oasis7.cognition.receipt-id.v1",
            &json!({"commit_id": expected_commit_id}),
        );
        let expected_receipt_digest = if marker.response_artifact_digest.is_empty() {
            cognition_digest_v1(
                "oasis7.cognition.receipt.v1",
                &json!({
                    "receipt_id": expected_receipt_id,
                    "envelope_digest": marker.envelope_digest,
                    "action_id": marker.action_id,
                    "world_root": marker.parent_world_hash,
                }),
            )
        } else {
            cognition_digest_v1(
                "oasis7.cognition.receipt.v1",
                &json!({
                    "receipt_id": expected_receipt_id,
                    "envelope_digest": marker.envelope_digest,
                    "action_id": marker.action_id,
                    "world_root": marker.parent_world_hash,
                    "response_artifact_digest": marker.response_artifact_digest,
                }),
            )
        };
        let expected_staged_event_root = cognition_digest_v1(
            "oasis7.cognition.staged-event-root.v1",
            &json!({
                "world_root": marker.parent_world_hash,
                "envelope_digest": marker.envelope_digest,
            }),
        );
        let expected_finality_binding = finality_binding_digest_v1(
            &marker.branch_id,
            marker.finality_epoch,
            marker.finality_block_hash.as_deref(),
            &marker.finality_status,
            marker.reorg_epoch,
        );
        let expected_feedback_id =
            cognition_digest_v1("oasis7.cognition.feedback-id.v1", &marker.envelope_digest);
        if marker.commit_id != expected_commit_id
            || marker.receipt_id != expected_receipt_id
            || marker.receipt_digest != expected_receipt_digest
            || marker.staged_event_root != expected_staged_event_root
            || marker.finality_binding_digest != expected_finality_binding
            || marker.feedback_id != expected_feedback_id
        {
            return Err(WorldError::DistributedValidationFailed {
                reason: "invalid_commit_record_digest".to_string(),
            });
        }
        if !marker.response_artifact_digest.is_empty()
            && (!canonical_cognition_digest(&marker.response_artifact_digest)
                || marker.response_context_discriminator
                    != RuntimeCognitionResponseArtifactV1::CONTEXT_DISCRIMINATOR
                || marker.response_context_version
                    != RuntimeCognitionResponseArtifactV1::CONTEXT_VERSION
                || marker.response_retry_seq == 0
                || marker.transport_attempt == 0)
        {
            return Err(WorldError::DistributedValidationFailed {
                reason: "invalid_response_artifact_binding".to_string(),
            });
        }
    }
    Ok(())
}

pub(super) fn marker_root_conflict(marker: &WorldCommitRecordV1, root: &WorldRootViewV1) -> bool {
    if root.world_id != marker.world_id || root.branch_id != marker.branch_id {
        return true;
    }
    // A committed marker is historical evidence after a later ordinary World
    // tick. The current root is intentionally different from the marker's
    // staged root in that case; `committed_marker_history_conflict` verifies
    // the durable receipt-link boundary for this historical branch.
    if marker.status == "committed" && root.logical_tick > marker.parent_tick {
        return false;
    }
    let parent = root.state_root == marker.parent_world_hash
        && root.logical_tick == marker.parent_tick
        && root.commit_id.is_none();
    let next = root.state_root == marker.staged_state_root
        && root.logical_tick == marker.parent_tick.saturating_add(1)
        && root.commit_id.as_deref() == Some(marker.commit_id.as_str());
    let stable = marker.status == "committed"
        && root.state_root == marker.staged_state_root
        && root.logical_tick == marker.parent_tick
        && root.commit_id.as_deref() == Some(marker.commit_id.as_str());
    !parent && !next && !stable
}

/// Verify that a committed marker has a durable receipt-link boundary. Once a
/// later ordinary tick advances the World root, the marker remains valid as
/// historical evidence only when this canonical link still proves its parent
/// and committed state roots. Older projections lack the optional boundary
/// fields; their receipt/link identity remains the compatibility proof.
pub(super) fn committed_marker_history_conflict(
    projection: &JsonValue,
    marker: &WorldCommitRecordV1,
    current_tick: u64,
) -> bool {
    if marker.status != "committed" || current_tick <= marker.parent_tick {
        return false;
    }
    let Some(events) = projection
        .get("cognition_journal")
        .and_then(|journal| journal.get("events"))
        .and_then(JsonValue::as_array)
    else {
        return true;
    };
    let Some(link) = events.iter().find(|event| {
        event.get("event_kind").and_then(JsonValue::as_str) == Some("WorldReceiptLinked")
            && event
                .get("envelope_idempotency_key")
                .and_then(JsonValue::as_str)
                == Some(marker.envelope_idempotency_key.as_str())
            && event.get("receipt_id").and_then(JsonValue::as_str)
                == Some(marker.receipt_id.as_str())
    }) else {
        return true;
    };
    let optional_matches =
        |field: &str, expected: &JsonValue| link.get(field).is_none_or(|value| value == expected);
    !optional_matches("commit_id", &json!(marker.commit_id))
        || !optional_matches("parent_tick", &json!(marker.parent_tick))
        || !optional_matches("parent_world_hash", &json!(marker.parent_world_hash))
        || !optional_matches("staged_state_root", &json!(marker.staged_state_root))
}

pub(super) fn visible_root_conflict(
    marker: &WorldCommitRecordV1,
    visible: &WorldRootViewV1,
    trusted_state_root: &str,
    trusted_tick: u64,
) -> bool {
    visible.world_id != marker.world_id
        || visible.branch_id != marker.branch_id
        || visible.state_root != trusted_state_root
        || visible.logical_tick != trusted_tick
        || visible.head_status != "canonical"
        || (marker.status == "committed"
            && visible.commit_id.as_deref() != Some(marker.commit_id.as_str()))
}

pub(super) fn has_receipt_conflict(
    receipts: &[CognitionReceiptViewV1],
    marker: &WorldCommitRecordV1,
) -> bool {
    receipts.iter().any(|receipt| {
        receipt.receipt_id == marker.receipt_id && receipt.receipt_digest != marker.receipt_digest
    })
}

pub(super) fn has_idempotency_conflict(
    index: &BTreeMap<String, IdempotencyProjection>,
    marker: &WorldCommitRecordV1,
) -> bool {
    index
        .get(&marker.envelope_idempotency_key)
        .is_some_and(|entry| {
            entry.envelope_digest != marker.envelope_digest
                || (entry.disposition == "committed" && entry.receipt_id != marker.receipt_id)
        })
}

pub(super) fn canonical_cognition_digest(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("blake3:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}
