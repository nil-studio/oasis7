use super::World;
use crate::service_admission::{
    SchedulerIntentV1, SchedulerOperationV1, SignedReadRequest, agent_authority,
    verify_read_request,
};
use serde::Serialize;
use serde_json::Value;

fn json<T: Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}
fn error(error: crate::WorldError) -> String {
    format!("{error:?}")
}

impl World {
    pub fn apply_authenticated_scheduler(
        &mut self,
        signed: SignedReadRequest<SchedulerIntentV1>,
    ) -> Result<Value, String> {
        self.apply_authenticated_scheduler_staged(signed, &[])
    }
    pub fn apply_authenticated_scheduler_staged(
        &mut self,
        signed: SignedReadRequest<SchedulerIntentV1>,
        staged: &[crate::service_admission::CanonicalIntentResultV1],
    ) -> Result<Value, String> {
        verify_read_request("scheduler", &signed)?;
        let task_context = match &signed.request.operation {
            SchedulerOperationV1::TaskProviderPrefix { context }
            | SchedulerOperationV1::TaskProviderFailure { context, .. } => Some(context),
            _ => None,
        };
        if let Some(context) = task_context {
            if let SchedulerOperationV1::TaskProviderFailure { reason, .. } =
                &signed.request.operation
            {
                crate::service_admission::game_task::validate_task_failure_context(
                    self,
                    context,
                    &signed.subject_public_key,
                    signed.request.delegation_generation,
                    reason,
                    staged,
                )?;
            } else {
                crate::service_admission::game_task::validate_task_context_identity(
                    self,
                    context,
                    &signed.subject_public_key,
                    signed.request.delegation_generation,
                    staged,
                )?;
            }
            oasis7_client_api::world_service::private_goal::verify_owner_signature_strict(
                "scheduler",
                &signed.request,
                &signed.subject_public_key,
                &signed.signature_hex,
            )?;
            if context.fence.agent_id != signed.request.agent_id {
                return Err("task context subject mismatch".into());
            }
            crate::service_admission::game_task::validate_cognition_fence_staged(
                self,
                &signed.request.agent_id,
                Some(&context.fence),
                &signed.subject_public_key,
                signed.request.delegation_generation,
                staged,
            )?;
        } else if matches!(
            &signed.request.operation,
            SchedulerOperationV1::ProviderPrefix { .. }
                | SchedulerOperationV1::ProviderFailure { .. }
        ) {
            crate::service_admission::game_task::validate_cognition_fence_staged(
                self,
                &signed.request.agent_id,
                None,
                &signed.subject_public_key,
                signed.request.delegation_generation,
                staged,
            )?;
        }
        let intent = signed.request;
        agent_authority::validate_agent_signer(
            self,
            &intent.agent_id,
            &signed.subject_public_key,
            intent.delegation_generation,
        )?;
        if self.current_cognition_base_binding().map_err(error)? != intent.captured_base_binding {
            return Err("scheduler base binding changed".into());
        }
        let agent = intent.agent_id.as_str();
        let identity = self
            .capability_revocation_state
            .agent_identities
            .get(agent)
            .ok_or("missing Agent identity")?
            .clone();
        // Existing operations run on a clone; publication failure cannot leave
        // partially mutated canonical scheduler/economy state.
        let mut transaction = self.clone();
        let result = match intent.operation {
            SchedulerOperationV1::TaskProviderPrefix { context } => {
                context.validate_capture_deadline(intent.captured_base_binding.base_tick)?;

                crate::service_admission::game_task::prepare_task_prefix(
                    &mut transaction,
                    &context,
                    staged,
                )?;
                let exists = |world: &World, kind: &str| {
                    world
                        .cognition()
                        .get("cognition_journal")
                        .and_then(|j| j.get("events"))
                        .and_then(Value::as_array)
                        .is_some_and(|events| {
                            events.iter().any(|e| {
                                e.get("event_kind").and_then(Value::as_str) == Some(kind)
                                    && e.get("agent_id").and_then(Value::as_str) == Some(agent)
                                    && e.get("decision_request_id").and_then(Value::as_str)
                                        == Some(&context.decision_request_id)
                                    && e.get("request_digest").and_then(Value::as_str)
                                        == Some(&context.request_commitment)
                            })
                        })
                };
                if !exists(&transaction, "TurnStarted") {
                    transaction
                        .start_cognition_turn(
                            agent,
                            &context.agent_session_id,
                            &context.agent_turn_id,
                            &context.decision_request_id,
                            &context.request_commitment,
                        )
                        .map_err(error)?;
                }
                if !exists(&transaction, "ContextCaptured") {
                    transaction
                        .capture_cognition_context(
                            agent,
                            &context.agent_session_id,
                            &context.agent_turn_id,
                            &context.decision_request_id,
                            &context.request_commitment,
                            &context.context_commitment,
                        )
                        .map_err(error)?;
                }
                transaction
                    .dispatch_cognition_request(
                        agent,
                        &context.agent_session_id,
                        &context.agent_turn_id,
                        &context.decision_request_id,
                        &context.request_commitment,
                        &crate::AgentDecisionEnvelopeV1::provider_invocation_key_for_request(&context.request_commitment),
                        context.retry_seq,
                        context.transport_attempt,
                    )
                    .map_err(error)?;
                serde_json::json!({"captured": true, "request_commitment": context.request_commitment})
            }
            SchedulerOperationV1::TaskProviderFailure { context, reason } => {
                if !matches!(
                    reason.as_str(),
                    "provider_failed"
                        | "transport_exhausted"
                        | "context_expired"
                        | "executor_aborted"
                ) {
                    return Err("task failure requires bounded public code".into());
                }
                transaction
                    .fail_cognition_turn(
                        agent,
                        &context.agent_session_id,
                        &context.agent_turn_id,
                        &context.decision_request_id,
                        &context.request_commitment,
                        "cognition_failed",
                        context.retry_seq,
                        context.transport_attempt,
                    )
                    .map_err(error)?;
                crate::service_admission::game_task::release_task_context_leases(
                    &mut transaction,
                    &context,
                )?;
                serde_json::json!({"failed": true, "request_commitment": context.request_commitment})
            }
            SchedulerOperationV1::ProviderFailure { request, reason } => {
                if request.agent_subject != agent {
                    return Err("provider failure subject mismatch".into());
                }
                request.validate().map_err(|e| e.to_string())?;
                transaction
                    .fail_cognition_turn(
                        agent,
                        &request.agent_session_id,
                        &request.agent_turn_id,
                        &request.decision_request_id,
                        &request.request_digest.to_string(),
                        &reason,
                        request.retry_seq,
                        request.transport_attempt,
                    )
                    .map_err(error)?;
                serde_json::json!({"failed": true, "request_digest": request.request_digest.to_string()})
            }
            SchedulerOperationV1::ReserveLease(request) => {
                if request.agent_id != agent || request.account_id != identity.owner_binding {
                    return Err("lease subject mismatch".into());
                }
                json(
                    transaction
                        .reserve_cognition_lease(request)
                        .map_err(error)?,
                )?
            }
            SchedulerOperationV1::SettleLease {
                lease_id,
                consumed_amount,
            } => {
                transaction.require_service_lease_owner(agent, &lease_id)?;
                json(
                    transaction
                        .settle_cognition_lease(&lease_id, consumed_amount)
                        .map_err(error)?,
                )?
            }
            SchedulerOperationV1::ReleaseLease { lease_id } => {
                transaction.require_service_lease_owner(agent, &lease_id)?;
                json(
                    transaction
                        .release_cognition_lease(&lease_id)
                        .map_err(error)?,
                )?
            }
            SchedulerOperationV1::AdmitContinuation(proposal) => {
                if proposal.agent_id != agent {
                    return Err("continuation subject mismatch".into());
                }
                json(
                    transaction
                        .admit_cognition_continuation(proposal)
                        .map_err(error)?,
                )?
            }
            SchedulerOperationV1::ConsumeContinuationBudget {
                continuation_id,
                budget_spent,
                current_context,
            } => {
                transaction.require_service_continuation_owner(agent, &continuation_id, false)?;
                json(
                    transaction
                        .consume_cognition_continuation_budget_with_context(
                            &continuation_id,
                            budget_spent,
                            current_context,
                        )
                        .map_err(error)?,
                )?
            }
            SchedulerOperationV1::TransitionContinuation {
                continuation_id,
                to,
                logical_tick,
            } => {
                transaction.require_service_continuation_owner(agent, &continuation_id, false)?;
                if logical_tick != transaction.state.time {
                    return Err("transition tick mismatch".into());
                }
                json(
                    transaction
                        .transition_cognition_continuation(&continuation_id, to, logical_tick)
                        .map_err(error)?,
                )?
            }
            SchedulerOperationV1::ResumeWake {
                wake_id,
                proposal,
                budget_spent,
                resume,
                current_context,
            } => {
                transaction.require_service_continuation_owner(agent, &wake_id, true)?;
                if proposal.agent_id != agent {
                    return Err("resume subject mismatch".into());
                }
                json(
                    transaction
                        .resume_cognition_wake_with_context(
                            &wake_id,
                            proposal,
                            budget_spent,
                            resume,
                            current_context,
                        )
                        .map_err(error)?,
                )?
            }
            SchedulerOperationV1::HandoffWake {
                wake_id,
                disposition,
                current_context,
            } => {
                transaction.require_service_continuation_owner(agent, &wake_id, true)?;
                // A completed action legitimately changes its observation.
                // Existing terminal handoff validates durable wake ownership
                // and terminal prerequisites without recapturing old context.
                let handoff = if matches!(
                    disposition,
                    crate::CognitionWakeDispositionV1::Terminal { .. }
                ) {
                    transaction.handoff_cognition_wake(&wake_id, disposition)
                } else {
                    transaction.handoff_cognition_wake_with_context(
                        &wake_id,
                        disposition,
                        current_context,
                    )
                };
                json(handoff.map_err(error)?)?
            }
            SchedulerOperationV1::ProviderPrefix {
                request,
                context_digest,
            } => {
                if request.agent_subject != agent {
                    return Err("provider subject mismatch".into());
                }
                request.validate().map_err(|e| e.to_string())?;
                let mut logical = request.clone();
                logical.transport_attempt = 1;
                if oasis7_agent_api::h_v1("oasis7.cognition.context.v1", &logical).to_string()
                    != context_digest
                {
                    return Err("provider context digest mismatch".into());
                }
                let exists = |world: &World, kind: &str| {
                    world
                        .cognition()
                        .get("cognition_journal")
                        .and_then(|j| j.get("events"))
                        .and_then(Value::as_array)
                        .is_some_and(|events| {
                            events.iter().any(|event| {
                                event.get("event_kind").and_then(Value::as_str) == Some(kind)
                                    && event.get("agent_id").and_then(Value::as_str) == Some(agent)
                                    && event.get("decision_request_id").and_then(Value::as_str)
                                        == Some(&request.decision_request_id)
                                    && event.get("request_digest").and_then(Value::as_str)
                                        == Some(request.request_digest.to_string().as_str())
                            })
                        })
                };
                let digest = request.request_digest.to_string();
                if !exists(&transaction, "TurnStarted") {
                    transaction
                        .start_cognition_turn(
                            agent,
                            &request.agent_session_id,
                            &request.agent_turn_id,
                            &request.decision_request_id,
                            &digest,
                        )
                        .map_err(error)?;
                }
                if !exists(&transaction, "ContextCaptured") {
                    transaction
                        .capture_cognition_context(
                            agent,
                            &request.agent_session_id,
                            &request.agent_turn_id,
                            &request.decision_request_id,
                            &digest,
                            &context_digest,
                        )
                        .map_err(error)?;
                }
                transaction
                    .dispatch_cognition_request(
                        agent,
                        &request.agent_session_id,
                        &request.agent_turn_id,
                        &request.decision_request_id,
                        &digest,
                        &request.provider_invocation_key().to_string(),
                        request.retry_seq,
                        request.transport_attempt,
                    )
                    .map_err(error)?;
                serde_json::json!({"dispatched": true, "request_digest": digest})
            }
        };
        *self = transaction;
        Ok(result)
    }

    fn require_service_lease_owner(&self, agent: &str, lease_id: &str) -> Result<(), String> {
        let economy = self.cognition_economy().map_err(error)?;
        if economy
            .leases
            .get(lease_id)
            .is_none_or(|lease| lease.agent_id != agent)
        {
            return Err("lease not authorized for Agent".into());
        }
        Ok(())
    }

    fn require_service_continuation_owner(
        &self,
        agent: &str,
        id: &str,
        wake: bool,
    ) -> Result<(), String> {
        let continuations = self.cognition_continuations();
        let matched = continuations.as_array().is_some_and(|entries| {
            entries.iter().any(|entry| {
                entry
                    .get(if wake { "wake_id" } else { "continuation_id" })
                    .and_then(Value::as_str)
                    == Some(id)
                    && entry.get("agent_id").and_then(Value::as_str) == Some(agent)
            })
        });
        if !matched {
            return Err("continuation not authorized for Agent".into());
        }
        Ok(())
    }
}
