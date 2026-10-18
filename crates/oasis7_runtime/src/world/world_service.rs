use super::World;
use crate::cognition_recovery::cognition_digest_v1;
use crate::service_admission::{
    AgentSignerDelegationChangeV1, CognitionIntentV1, SignedReadRequest, agent_authority,
};
use crate::{RuntimeCognitionCommitRequestV1, WorldError};
use serde_json::json;
#[path = "world_service_scheduler.rs"]
mod scheduler;

pub(super) fn apply_service_authorization_event(
    world: &mut World,
    event: &crate::CapabilityAuthorizationEvent,
) -> Result<(), crate::WorldError> {
    use crate::CapabilityAuthorizationEvent as Event;
    let deny = |reason: String| crate::WorldError::DistributedValidationFailed { reason };
    match event {
        Event::AgentSignerDelegationInstalled { signed } => {
            agent_authority::validate_delegation(world, signed).map_err(deny)?;
            world
                .capability_revocation_state
                .agent_signer_delegations
                .insert(signed.request.agent_id.clone(), signed.request.clone());
        }
        Event::WorldServiceIntentRecorded { key, record } => {
            crate::service_admission::correlation::validate_result(key, record)
                .map_err(deny)?;
            if world
                .capability_revocation_state
                .world_service_results
                .get(key)
                .is_some_and(|previous| previous != record)
            {
                return Err(deny("canonical intent result conflict".into()));
            }
            world
                .capability_revocation_state
                .world_service_results
                .insert(key.clone(), record.clone());
        }
        _ => return Err(deny("unsupported service authorization event".into())),
    }
    Ok(())
}

impl World {
    pub fn service_continuation_context(&self, continuation_id: &str) -> Option<serde_json::Value> {
        self.cognition
            .get("continuation_contexts")?
            .get(continuation_id)
            .cloned()
    }
    pub fn record_world_service_result(
        &mut self,
        result: crate::service_admission::CanonicalIntentResultV1,
    ) -> Result<(), String> {
        let key = crate::service_admission::correlation::key_digest(
            &result.request.correlation.key,
        )?;
        let record = serde_json::to_value(result).map_err(|e| e.to_string())?;
        self.append_capability_authorization_event_batch(vec![
            crate::CapabilityAuthorizationEvent::WorldServiceIntentRecorded {
                key,
                record,
            },
        ])
        .map(|_| ())
        .map_err(|e| format!("{e:?}"))
    }
    /// Called only by registered canonical execution; HTTP admission uses the
    /// pure verifier and never invokes this mutation.
    pub fn apply_agent_signer_delegation(
        &mut self,
        signed: &SignedReadRequest<AgentSignerDelegationChangeV1>,
    ) -> Result<(), String> {
        agent_authority::validate_delegation(self, signed)?;
        if self
            .capability_revocation_state
            .agent_signer_delegations
            .get(&signed.request.agent_id)
            == Some(&signed.request)
        {
            return Ok(());
        }
        self.append_capability_authorization_event_batch(vec![
            crate::CapabilityAuthorizationEvent::AgentSignerDelegationInstalled {
                signed: signed.clone(),
            },
        ])
        .map_err(|e| format!("{e:?}"))?;
        Ok(())
    }

    pub fn apply_authenticated_feedback_ack(
        &mut self,
        signed: SignedReadRequest<crate::service_admission::FeedbackAckIntentV1>,
    ) -> Result<serde_json::Value, String> {
        agent_authority::validate_feedback_ack(self, &signed)?;
        let ack = signed.request;
        let record = self
            .runtime_feedback_outbox()
            .map_err(|e| format!("{e:?}"))?
            .into_iter()
            .find(|r| r.feedback_id == ack.feedback_id)
            .ok_or("canonical feedback acknowledgement record missing")?;
        // Native claim/ack events are published in one candidate transaction;
        // never assign delivery state on a caller's sanitized projection.
        let mut transaction = self.clone();
        if record.state != "acked" {
            transaction
                .claim_runtime_feedback(&ack.feedback_id)
                .map_err(|e| format!("{e:?}"))?;
            transaction
                .ack_runtime_feedback(&ack.feedback_id)
                .map_err(|e| format!("{e:?}"))?;
        }
        *self = transaction;
        Ok(serde_json::json!({"acknowledged": ack, "delivery_state": "acked"}))
    }

    /// Exact Runtime identity shared by commit evaluation and the narrow
    /// authenticated service rejection publisher. It grants no admission.
    pub(crate) fn stale_cognition_rejection_identity(
        request: &RuntimeCognitionCommitRequestV1,
        action: &crate::Action,
    ) -> Result<(String, String), WorldError> {
        let action_value = serde_json::to_value(action).map_err(WorldError::from)?;
        let digest = cognition_digest_v1(
            "oasis7.cognition.stale-envelope.v1",
            &json!({"request": request, "action": action_value}),
        );
        let key = cognition_digest_v1(
            "oasis7.cognition.stale-idempotency.v1",
            &json!({"request_digest": &request.request_digest, "envelope_digest": &digest}),
        );
        Ok((digest, key))
    }

    /// Only a token minted after authenticated, actually evaluated stale
    /// cognition may close that exact turn and publish rejection feedback.
    pub(crate) fn publish_authenticated_stale_cognition_rejection(
        &mut self,
        rejection: crate::cognition_disposition::ValidatedStaleCognitionRejection,
    ) -> Result<(), String> {
        let (feedback, envelope_digest, idempotency_key) = rejection.into_parts();
        let mut transaction = self.clone();
        transaction
            .record_stale_cognition_rejection(
                &feedback.agent_subject,
                &feedback.agent_session_id,
                &feedback.agent_turn_id,
                &feedback.decision_request_id,
                &feedback.request_digest,
                &envelope_digest,
                &idempotency_key,
            )
            .map_err(|_| "canonical rejected cognition terminal publication failed")?;
        transaction
            .allocate_runtime_feedback_with_projection(feedback, Default::default())
            .map_err(|_| "canonical rejected cognition feedback publication failed")?;
        *self = transaction;
        Ok(())
    }

    pub fn commit_authenticated_cognition(
        &mut self,
        signed: SignedReadRequest<CognitionIntentV1>,
    ) -> Result<serde_json::Value, String> {
        self.commit_authenticated_cognition_staged(signed, &[])
    }
    pub fn commit_authenticated_cognition_staged(
        &mut self,
        signed: SignedReadRequest<CognitionIntentV1>,
        staged: &[crate::service_admission::CanonicalIntentResultV1],
    ) -> Result<serde_json::Value, String> {
        match self.commit_authenticated_cognition_disposition_staged(signed, staged)? {
            crate::cognition_disposition::AuthenticatedCognitionDisposition::Committed(receipt) => {
                Ok(receipt)
            }
            crate::cognition_disposition::AuthenticatedCognitionDisposition::RejectedStale(rejection) => {
                Err(rejection.rejection_text())
            }
        }
    }
    /// Canonical adapters may preserve ONLY approved rejection metadata after
    /// discarding this candidate. Every other validation failure stays an error.
    pub fn commit_authenticated_cognition_disposition_staged(
        &mut self,
        signed: SignedReadRequest<CognitionIntentV1>,
        staged: &[crate::service_admission::CanonicalIntentResultV1],
    ) -> Result<crate::cognition_disposition::AuthenticatedCognitionDisposition, String>
    {
        agent_authority::validate_cognition_staged(self, &signed, staged)?;
        let subject = signed.subject_public_key;
        let intent = signed.request;
        // The outer service correlation already binds the signed payload. This
        // additional canonical fence prevents a differently signed explanation
        // from reusing a cognition decision whose request digest predates it.
        let identity = serde_json::json!([
            intent.request.agent_id,
            intent.request.agent_session_id,
            intent.request.agent_turn_id,
            intent.request.decision_request_id,
            intent.request.request_digest,
        ]);
        let key = identity.to_string();
        let proposal = serde_json::to_value(&intent.causal_proposal).map_err(|e| e.to_string())?;
        let stored = self
            .cognition
            .get("service_causal_proposals")
            .and_then(|map| map.get(&key));
        if stored.is_some_and(|old| old != &proposal) {
            return Err("canonical cognition causal proposal idempotency conflict".into());
        }
        let committed = self
            .cognition
            .get("commit_records")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|records| {
                records.iter().any(|record| {
                    record.get("agent_id") == Some(&identity[0])
                        && record.get("agent_session_id") == Some(&identity[1])
                        && record.get("agent_turn_id") == Some(&identity[2])
                        && record.get("decision_request_id") == Some(&identity[3])
                        && record.get("request_digest") == Some(&identity[4])
                        && record.get("status").and_then(serde_json::Value::as_str)
                            == Some("committed")
                })
            });
        // A legacy commit has no signed causal proposal. It cannot be upgraded
        // retroactively by a replay that introduces a new explanation.
        if committed && stored.is_none() && intent.causal_proposal.is_some() {
            return Err("canonical cognition causal proposal missing for prior commit".into());
        }
        if !committed {
            let mut context = crate::AgentDecisionCausalContextV1::default();
            if let Some(public) = &intent.causal_proposal {
                context.expected_consequence = public.expected_consequence.clone();
                context.stakes = public.stakes.clone();
                context.alternative = public.alternative.clone();
                context.reason = public.reason.clone();
                context.evidence_refs = public.evidence_refs.clone();
                context.correction_refs = public.correction_refs.clone();
                context.dissent = public.dissent.clone();
            }
            context.intent_id = self
                .state()
                .agents
                .get(&intent.request.agent_id)
                .and_then(|agent| agent.intent.as_ref())
                .filter(|intent| matches!(intent.status.as_str(), "accepted" | "blocked"))
                .map(|intent| intent.intent_id.clone());
            self.bind_agent_causal_decision(
                &intent.request.decision_request_id,
                &intent.request.agent_id,
                &intent.action,
                context,
            )
            .map_err(|e| format!("{e:?}"))?;
        }
        // The legacy commit checks action identity after base concurrency. A
        // stale result must not promote an unrelated actor into feedback.
        let actor_matches = intent.action.actor_id() == Some(intent.request.agent_id.as_str());
        // This publication identity belongs ONLY to the early captured-base
        // mismatch branch, never a later prepared-envelope stale error.
        let captured_base_stale = self
            .current_cognition_base_binding()
            .ok()
            .is_some_and(|current| intent.request.captured_base_binding != current);
        let rejected_metadata =
            crate::cognition_disposition::ValidatedStaleCognitionRejection::from_evaluated_request(
                &intent.request,
                &intent.action,
            )?;
        let committed = if let Some(fence) = &intent.task_fence {
            self.commit_task_cognition_action(
                intent.request,
                intent.action,
                intent.response_artifact,
                (fence, &subject, intent.delegation_generation),
                staged,
            )
        } else {
            self.commit_cognition_action(intent.request, intent.action, intent.response_artifact)
        };
        let (record, lineage) = match committed {
            Ok(committed) => committed,
            Err(crate::WorldError::DistributedValidationFailed { reason })
                if actor_matches
                    && captured_base_stale
                    && reason
                        == crate::CognitionCommitRejectReasonV1::StaleBase.code() =>
            {
                return Ok(
                    crate::cognition_disposition::AuthenticatedCognitionDisposition::RejectedStale(
                        rejected_metadata,
                    ),
                );
            }
            Err(error) => return Err(format!("{error:?}")),
        };
        self.verify_runtime_receipt_lineage(&lineage)
            .map_err(|e| format!("{e:?}"))?;
        let action_id = lineage
            .action_id
            .strip_prefix("action:")
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or("canonical receipt action identity invalid")?;
        // Match the local provider path: the World allocator verifies this
        // committed lineage and durably assigns the feedback sequence. Exact
        // replays return the same outbox entry, never a caller-minted receipt.
        let feedback = self
            .allocate_runtime_feedback_with_projection(
                crate::RuntimeFeedbackRequestV1 {
                    feedback_id: Some(lineage.feedback_id.clone()),
                    agent_subject: lineage.agent_id.clone(),
                    agent_session_id: lineage.agent_session_id.clone(),
                    agent_turn_id: lineage.agent_turn_id.clone(),
                    decision_request_id: lineage.decision_request_id.clone(),
                    candidate_action_id: Some(action_id),
                    runtime_receipt_id: Some(lineage.receipt_id.clone()),
                    status: lineage.status.clone(),
                    request_digest: lineage.request_digest.clone(),
                    reject_reason: None,
                },
                crate::RuntimeFeedbackProjectionV1 {
                    envelope_digest: Some(lineage.envelope_digest.clone()),
                    ..Default::default()
                },
            )
            .map_err(|e| format!("{e:?}"))?;
        feedback.validate().map_err(|e| e.to_string())?;
        // This is persisted in the same canonical candidate as the delegation
        // decision, cognition receipt and feedback; rejected candidates vanish.
        let object = self
            .cognition
            .as_object_mut()
            .ok_or("canonical cognition projection invalid")?;
        let proposals = object
            .entry("service_causal_proposals")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or("canonical causal proposal index invalid")?;
        proposals.insert(key, proposal);
        Ok(
            crate::cognition_disposition::AuthenticatedCognitionDisposition::Committed(
                serde_json::json!({"commit_record": record, "lineage": lineage, "feedback": feedback.payload}),
            ),
        )
    }
}
