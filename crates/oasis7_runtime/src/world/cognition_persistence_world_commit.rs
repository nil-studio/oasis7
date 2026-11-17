//! The original World cognition commit and stale-rejection mutation methods.
use super::*;

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
            validate_existing_retry_action_with_work_v1(
                existing,
                stored_action_digest.as_deref(),
                &action_digest,
                &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1,
            )
            .expect("legacy cognition retry work is infallible")
            .map_err(cognition_validation)?;
            let response_matches = parsed.responses.iter().any(|response| {
                response_artifact_matches_with_work_v1(
                    response,
                    &existing.envelope_digest,
                    &response_artifact_value,
                    &oasis7_wasm_abi::LegacyCapabilityCanonicalWorkV1,
                )
                .expect("legacy cognition response comparison work is infallible")
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
            schema_version: crate::cognition::AGENT_DECISION_ENVELOPE_V1_SCHEMA.to_string(),
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
