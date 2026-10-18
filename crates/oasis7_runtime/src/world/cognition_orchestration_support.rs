use super::super::cognition_persistence_validation::strict_optional_finality_hash;
use super::World;
use crate::cognition_recovery::{WorldCommitRecordV1, cognition_digest_v1};
use crate::cognition_retention::CognitionRetentionStore;
use crate::cognition_wake::{AgentContinuation, CognitionContinuationProposalV1};
use crate::error::WorldError;
use serde_json::{Value as JsonValue, json};
use std::collections::BTreeSet;

impl World {
    pub(super) fn bind_cognition_proposal_fields(
        &mut self,
        proposal: &mut CognitionContinuationProposalV1,
    ) -> Result<(), WorldError> {
        let manifest_hash = self.current_manifest_hash()?;
        if proposal.runtime_manifest_hash.is_empty() {
            proposal.runtime_manifest_hash = manifest_hash.clone();
        } else if proposal.runtime_manifest_hash != manifest_hash {
            return Err(super::cognition_validation_error(
                "runtime_manifest_mismatch",
            ));
        }
        let Some(binding) = self.cognition.get("runtime_binding") else {
            return Err(super::cognition_validation_error(
                "runtime_binding_required",
            ));
        };
        if proposal.branch_id.is_empty() {
            proposal.branch_id = binding["branch_id"].as_str().unwrap_or("main").to_string();
        }
        if proposal.finality_status.is_empty() {
            proposal.finality_status = binding["finality_status"]
                .as_str()
                .unwrap_or("pending")
                .to_string();
        }
        if proposal.finality_epoch == 0 {
            proposal.finality_epoch = binding["finality_epoch"].as_u64().unwrap_or_default();
        }
        if proposal.reorg_epoch == 0 {
            proposal.reorg_epoch = binding["reorg_epoch"].as_u64().unwrap_or_default();
        }
        if proposal.finality_block_hash.is_none() {
            proposal.finality_block_hash =
                strict_optional_finality_hash(binding.get("finality_block_hash"))?;
        }
        let block_hash = strict_optional_finality_hash(binding.get("finality_block_hash"))?;
        if binding["world_id"].as_str() != Some(proposal.world_id.as_str())
            || binding["branch_id"].as_str() != Some(proposal.branch_id.as_str())
            || binding["finality_epoch"].as_u64() != Some(proposal.finality_epoch)
            || block_hash != proposal.finality_block_hash
            || binding["finality_status"].as_str() != Some(proposal.finality_status.as_str())
            || binding["reorg_epoch"].as_u64() != Some(proposal.reorg_epoch)
        {
            return Err(super::cognition_validation_error(
                "foreign_continuation_proposal",
            ));
        }
        if !self.state.agents.is_empty() && !self.state.agents.contains_key(&proposal.agent_id) {
            return Err(super::cognition_validation_error(
                "continuation_agent_missing",
            ));
        }
        Ok(())
    }

    pub(super) fn validate_cognition_proposal_binding(
        &self,
        proposal: &CognitionContinuationProposalV1,
    ) -> Result<(), WorldError> {
        if proposal.runtime_manifest_hash != self.current_manifest_hash()? {
            return Err(super::cognition_validation_error(
                "runtime_manifest_mismatch",
            ));
        }
        if let Some(binding) = self.cognition.get("runtime_binding") {
            let block_hash = strict_optional_finality_hash(binding.get("finality_block_hash"))?;
            if binding["world_id"].as_str() != Some(proposal.world_id.as_str())
                || binding["branch_id"].as_str() != Some(proposal.branch_id.as_str())
                || binding["finality_epoch"].as_u64() != Some(proposal.finality_epoch)
                || block_hash != proposal.finality_block_hash
                || binding["finality_status"].as_str() != Some(proposal.finality_status.as_str())
                || binding["reorg_epoch"].as_u64() != Some(proposal.reorg_epoch)
            {
                return Err(super::cognition_validation_error(
                    "foreign_continuation_proposal",
                ));
            }
        }
        if !self.state.agents.is_empty() && !self.state.agents.contains_key(&proposal.agent_id) {
            return Err(super::cognition_validation_error(
                "continuation_agent_missing",
            ));
        }
        Ok(())
    }

    pub(super) fn cognition_turn_is_registered(
        &self,
        agent_id: &str,
        agent_session_id: &str,
        agent_turn_id: &str,
        decision_request_id: &str,
        request_digest: &str,
    ) -> bool {
        self.cognition
            .get("cognition_journal")
            .and_then(|journal| journal.get("events"))
            .and_then(JsonValue::as_array)
            .is_some_and(|events| {
                events.iter().any(|event| {
                    matches!(
                        event.get("kind").and_then(JsonValue::as_str),
                        Some("TurnStarted")
                            | Some("ContextCaptured")
                            | Some("RequestDispatched")
                            | Some("DecisionValidated")
                    ) && event.get("agent_id").and_then(JsonValue::as_str) == Some(agent_id)
                        && event.get("agent_session_id").and_then(JsonValue::as_str)
                            == Some(agent_session_id)
                        && event.get("agent_turn_id").and_then(JsonValue::as_str)
                            == Some(agent_turn_id)
                        && event.get("decision_request_id").and_then(JsonValue::as_str)
                            == Some(decision_request_id)
                        // An initial turn matches its own request digest. A
                        // continuation wake creates a new logical request
                        // digest because the Runtime continuation context is
                        // part of the next Harness request identity; its
                        // TurnStarted event therefore carries the stable
                        // origin digest for admission correlation.
                        && (event.get("request_digest").and_then(JsonValue::as_str)
                            == Some(request_digest)
                            || event
                                .get("origin_request_digest")
                                .and_then(JsonValue::as_str)
                                == Some(request_digest))
                })
            })
    }

    pub(in crate::world) fn cognition_continuations_typed(
        &self,
    ) -> Result<Vec<AgentContinuation>, WorldError> {
        let value = self
            .cognition
            .get("continuations")
            .cloned()
            .unwrap_or_else(|| JsonValue::Array(Vec::new()));
        serde_json::from_value(value).map_err(WorldError::from)
    }

    pub(super) fn cognition_set_continuations(
        &mut self,
        continuations: &[AgentContinuation],
    ) -> Result<(), WorldError> {
        let mut projection = self.cognition.as_object().cloned().unwrap_or_default();
        projection.insert(
            "continuations".to_string(),
            serde_json::to_value(continuations).map_err(WorldError::from)?,
        );
        self.cognition = JsonValue::Object(projection);
        Ok(())
    }

    pub(super) fn cognition_retention_store(&self) -> Result<CognitionRetentionStore, WorldError> {
        let Some(value) = self.cognition.get("retention_state") else {
            return Ok(CognitionRetentionStore::default());
        };
        serde_json::from_value(value.clone()).map_err(WorldError::from)
    }

    pub(super) fn cognition_set_retention_store(
        &mut self,
        store: &CognitionRetentionStore,
    ) -> Result<(), WorldError> {
        let mut projection = self.cognition.as_object().cloned().unwrap_or_default();
        projection.insert(
            "retention_state".to_string(),
            serde_json::to_value(store).map_err(WorldError::from)?,
        );
        self.cognition = JsonValue::Object(projection);
        Ok(())
    }

    /// Upgrade canonical terminal markers written before `retention_state`
    /// existed and protect the dense journal while an active continuation
    /// still addresses one of its event digests. Because every later event
    /// digest includes its parent, retaining only the referenced lifecycle is
    /// insufficient: compaction of any earlier lifecycle would invalidate the
    /// reference. Pinning the terminal marker set preserves the whole chain
    /// until the continuation becomes terminal.
    pub(super) fn migrate_cognition_retention_and_pin_wake_events(
        &self,
        store: &mut CognitionRetentionStore,
    ) -> Result<(), WorldError> {
        let markers: Vec<WorldCommitRecordV1> = self
            .cognition
            .get("commit_records")
            .and_then(JsonValue::as_array)
            .into_iter()
            .flatten()
            .map(|record| {
                serde_json::from_value(record.clone())
                    .map_err(|_| super::cognition_validation_error("invalid_cognition_projection"))
            })
            .collect::<Result<_, _>>()?;
        for marker in &markers {
            if matches!(marker.status.as_str(), "committed" | "aborted")
                && !store.contains_key(&marker.envelope_idempotency_key)
            {
                store.insert_commit_record(marker);
            }
        }
        store.clear_references_with_prefix("continuation-event:");

        let journal_events = self
            .cognition
            .get("cognition_journal")
            .and_then(|journal| journal.get("events"))
            .and_then(JsonValue::as_array)
            .cloned()
            .unwrap_or_default();
        let journal_sequences: std::collections::BTreeMap<&str, u64> = journal_events
            .iter()
            .filter_map(|event| {
                Some((
                    event.get("event_digest")?.as_str()?,
                    event.get("journal_seq")?.as_u64()?,
                ))
            })
            .collect();
        let active_event_references: Vec<(String, u64)> = self
            .cognition
            .get("continuations")
            .and_then(JsonValue::as_array)
            .into_iter()
            .flatten()
            .filter(|continuation| {
                !matches!(
                    continuation.get("status").and_then(JsonValue::as_str),
                    Some(
                        "completed"
                            | "cancelled"
                            | "invalidated"
                            | "expired"
                            | "rejected"
                            | "Completed"
                            | "Cancelled"
                            | "Invalidated"
                            | "Expired"
                            | "Rejected"
                    )
                )
            })
            .filter_map(|continuation| {
                let referenced_seq = continuation
                    .get("wake_conditions")
                    .and_then(JsonValue::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|condition| {
                        condition.get("event_digest").and_then(JsonValue::as_str)
                    })
                    .filter_map(|digest| journal_sequences.get(digest).copied())
                    .max();
                referenced_seq.map(|sequence| {
                    let reference = continuation
                        .get("continuation_id")
                        .and_then(JsonValue::as_str)
                        .unwrap_or("active-continuation")
                        .to_string();
                    (reference, sequence)
                })
            })
            .collect();
        for (reference, referenced_seq) in active_event_references {
            for marker in &markers {
                let marker_has_prefix_event = journal_events.iter().any(|event| {
                    event
                        .get("journal_seq")
                        .and_then(JsonValue::as_u64)
                        .is_some_and(|sequence| sequence <= referenced_seq)
                        && (event
                            .get("envelope_idempotency_key")
                            .and_then(JsonValue::as_str)
                            == Some(marker.envelope_idempotency_key.as_str())
                            || (!marker.agent_id.is_empty()
                                && event.get("agent_id").and_then(JsonValue::as_str)
                                    == Some(marker.agent_id.as_str())
                                && event.get("agent_session_id").and_then(JsonValue::as_str)
                                    == Some(marker.agent_session_id.as_str())
                                && event.get("agent_turn_id").and_then(JsonValue::as_str)
                                    == Some(marker.agent_turn_id.as_str())
                                && event.get("decision_request_id").and_then(JsonValue::as_str)
                                    == Some(marker.decision_request_id.as_str())))
                });
                if matches!(marker.status.as_str(), "committed" | "aborted")
                    && marker_has_prefix_event
                {
                    store.pin_reference(
                        &marker.envelope_idempotency_key,
                        &format!("continuation-event:{reference}"),
                    );
                }
            }
        }
        Ok(())
    }

    /// Remove expired terminal cognition records from every canonical
    /// projection in one World transaction. The cognition journal is a dense
    /// chain, so retained events are re-linked after deleting complete
    /// terminal lifecycles; payload digests remain unchanged while sequence
    /// and parent linkage are rebuilt deterministically.
    pub(super) fn compact_cognition_projection(
        &mut self,
        deleted_keys: &[String],
    ) -> Result<(), WorldError> {
        if deleted_keys.is_empty() {
            return Ok(());
        }
        let deleted_keys: BTreeSet<&str> = deleted_keys.iter().map(String::as_str).collect();
        let projection = self
            .cognition
            .as_object_mut()
            .ok_or_else(|| super::cognition_validation_error("cognition_projection_not_object"))?;
        let deleted_markers: Vec<WorldCommitRecordV1> = projection
            .get("commit_records")
            .and_then(JsonValue::as_array)
            .into_iter()
            .flatten()
            .filter(|record| {
                record
                    .get("envelope_idempotency_key")
                    .and_then(JsonValue::as_str)
                    .is_some_and(|key| deleted_keys.contains(key))
            })
            .map(|record| {
                serde_json::from_value(record.clone())
                    .map_err(|_| super::cognition_validation_error("invalid_cognition_projection"))
            })
            .collect::<Result<_, _>>()?;
        if deleted_markers.is_empty() {
            // Explicit terminal records that do not have a corresponding
            // canonical commit marker are still valid retention entries, but
            // there is no canonical projection to compact for them.
            return Ok(());
        }

        let deleted_commit_ids: BTreeSet<&str> = deleted_markers
            .iter()
            .map(|marker| marker.commit_id.as_str())
            .collect();
        let deleted_receipt_ids: BTreeSet<&str> = deleted_markers
            .iter()
            .filter(|marker| marker.status == "committed")
            .map(|marker| marker.receipt_id.as_str())
            .collect();
        let event_belongs_to_deleted_marker = |event: &JsonValue| {
            let key = event
                .get("envelope_idempotency_key")
                .and_then(JsonValue::as_str);
            deleted_markers.iter().any(|marker| {
                key == Some(marker.envelope_idempotency_key.as_str())
                    || (matches!(
                        event.get("event_kind").and_then(JsonValue::as_str),
                        Some("TurnStarted") | Some("ContextCaptured") | Some("RequestDispatched")
                    ) && !marker.agent_id.is_empty()
                        && event.get("agent_id").and_then(JsonValue::as_str)
                            == Some(marker.agent_id.as_str())
                        && event.get("agent_session_id").and_then(JsonValue::as_str)
                            == Some(marker.agent_session_id.as_str())
                        && event.get("agent_turn_id").and_then(JsonValue::as_str)
                            == Some(marker.agent_turn_id.as_str())
                        && event.get("decision_request_id").and_then(JsonValue::as_str)
                            == Some(marker.decision_request_id.as_str()))
            })
        };

        if let Some(records) = projection
            .get_mut("commit_records")
            .and_then(JsonValue::as_array_mut)
        {
            records.retain(|record| {
                record
                    .get("envelope_idempotency_key")
                    .and_then(JsonValue::as_str)
                    .is_none_or(|key| !deleted_keys.contains(key))
            });
        }
        if let Some(responses) = projection
            .get_mut("responses")
            .and_then(JsonValue::as_array_mut)
        {
            responses.retain(|response| {
                response
                    .get("envelope_digest")
                    .and_then(JsonValue::as_str)
                    .is_none_or(|digest| {
                        !deleted_markers
                            .iter()
                            .any(|marker| marker.envelope_digest == digest)
                    })
            });
        }
        if let Some(receipts) = projection
            .get_mut("receipt_registry")
            .and_then(JsonValue::as_array_mut)
        {
            receipts.retain(|receipt| {
                receipt
                    .get("receipt_id")
                    .and_then(JsonValue::as_str)
                    .is_none_or(|receipt_id| !deleted_receipt_ids.contains(receipt_id))
            });
        }
        if let Some(lineages) = projection
            .get_mut("receipt_lineage_registry")
            .and_then(JsonValue::as_array_mut)
        {
            lineages.retain(|lineage| {
                lineage
                    .get("receipt_id")
                    .and_then(JsonValue::as_str)
                    .is_none_or(|receipt_id| !deleted_receipt_ids.contains(receipt_id))
            });
        }
        if let Some(index) = projection
            .get_mut("idempotency_index")
            .and_then(JsonValue::as_object_mut)
        {
            index.retain(|key, _| !deleted_keys.contains(key.as_str()));
        }
        if let Some(action_digests) = projection
            .get_mut("action_digests")
            .and_then(JsonValue::as_object_mut)
        {
            action_digests.retain(|commit_id, _| !deleted_commit_ids.contains(commit_id.as_str()));
        }
        if let Some(staged_actions) = projection
            .get_mut("staged_actions")
            .and_then(JsonValue::as_object_mut)
        {
            staged_actions.retain(|commit_id, _| !deleted_commit_ids.contains(commit_id.as_str()));
        }

        let journal = projection
            .get_mut("cognition_journal")
            .ok_or_else(|| super::cognition_validation_error("cognition_journal_missing"))?
            .as_object_mut()
            .ok_or_else(|| super::cognition_validation_error("cognition_journal_not_object"))?;
        let (retained_events, head_seq, head_digest) = {
            let events = journal
                .get_mut("events")
                .ok_or_else(|| super::cognition_validation_error("cognition_events_missing"))?
                .as_array_mut()
                .ok_or_else(|| super::cognition_validation_error("cognition_events_not_array"))?;
            events.retain(|event| !event_belongs_to_deleted_marker(event));

            let mut parent_digest = String::new();
            for (index, event) in events.iter_mut().enumerate() {
                let object = event.as_object_mut().ok_or_else(|| {
                    super::cognition_validation_error("cognition_event_not_object")
                })?;
                let sequence = index.saturating_add(1) as u64;
                object.insert("journal_seq".to_string(), json!(sequence));
                object.insert("parent_event_digest".to_string(), json!(&parent_digest));
                object.remove("event_digest");
                let event_digest = cognition_digest_v1(
                    "oasis7.cognition.event.v1",
                    &JsonValue::Object(object.clone()),
                );
                object.insert("event_digest".to_string(), json!(&event_digest));
                parent_digest = event_digest;
            }
            let head_seq = events
                .last()
                .and_then(|event| event.get("journal_seq"))
                .and_then(JsonValue::as_u64)
                .unwrap_or_default();
            let retained_events = events.clone();
            let head_digest = cognition_digest_v1(
                "oasis7.cognition.journal-head.v1",
                &json!({"head_seq": head_seq, "events": retained_events}),
            );
            (retained_events, head_seq, head_digest)
        };
        journal.insert("head_seq".to_string(), json!(head_seq));
        journal.insert("head_digest".to_string(), json!(&head_digest));
        let journal_head = json!(head_digest);

        // Markers retain the sequence of their DecisionValidated event after
        // compaction. Responses point at the new canonical head so restore
        // can validate them without requiring deleted historical prefixes.
        if let Some(records) = projection
            .get_mut("commit_records")
            .and_then(JsonValue::as_array_mut)
        {
            for record in records {
                let Some(marker) =
                    serde_json::from_value::<WorldCommitRecordV1>(record.clone()).ok()
                else {
                    continue;
                };
                if let Some(event) = retained_events.iter().find(|event| {
                    event.get("event_kind").and_then(JsonValue::as_str) == Some("DecisionValidated")
                        && event
                            .get("envelope_idempotency_key")
                            .and_then(JsonValue::as_str)
                            == Some(marker.envelope_idempotency_key.as_str())
                }) && let Some(sequence) = event.get("journal_seq").and_then(JsonValue::as_u64)
                {
                    record["cognition_journal_seq"] = json!(sequence);
                }
            }
        }
        if let Some(responses) = projection
            .get_mut("responses")
            .and_then(JsonValue::as_array_mut)
        {
            for response in responses {
                response["journal_head"] = journal_head.clone();
            }
        }
        if let Some(recovery) = projection
            .get_mut("recovery")
            .and_then(JsonValue::as_object_mut)
        {
            recovery.insert("journal_head".to_string(), journal_head);
        }
        Ok(())
    }
}
