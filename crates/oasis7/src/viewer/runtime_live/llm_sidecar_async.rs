use super::super::decision_trace::{is_budget_exhausted_wait, is_trace_only_overflow};
use super::*;

#[cfg(not(target_arch = "wasm32"))]
use crate::simulator::{AsyncAgentTurnOutcome, AsyncTurnLifecycle};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(in crate::viewer::runtime_live) struct RuntimeLlmDecision {
    pub(in crate::viewer::runtime_live) agent_id: String,
    pub(in crate::viewer::runtime_live) decision: AgentDecision,
    pub(in crate::viewer::runtime_live) decision_trace: Option<AgentDecisionTrace>,
    pub(in crate::viewer::runtime_live) cognition: Option<RuntimeProviderActionContext>,
    /// Provider memory intents remain attached to the response until the
    /// Runtime receipt/readback gate accepts them.
    #[serde(default)]
    pub(in crate::viewer::runtime_live) memory_write_intents:
        Vec<crate::simulator::MemoryWriteIntent>,
    /// True only when a normal provider Wait was admitted through both the
    /// Harness and Runtime durable continuation seams. The control plane must
    /// not fall back to a local timer for this case.
    #[serde(default)]
    pub(in crate::viewer::runtime_live) continuation_admitted: bool,
}

pub(super) fn provider_trace_retryable(trace: &AgentDecisionTrace) -> bool {
    trace
        .llm_output
        .as_ref()
        .and_then(|output| serde_json::from_str::<serde_json::Value>(output).ok())
        .and_then(|output| {
            output
                .get("provider_error")
                .or_else(|| output.get("trace_payload")?.get("provider_error"))
                .and_then(|error| error.get("retryable"))
                .and_then(serde_json::Value::as_bool)
        })
        .unwrap_or(false)
}

/// Context capture identifies the logical request, not one transport attempt.
/// Normalize the attempt before hashing so a retry can append only a new
/// `RequestDispatched` event while reusing the already captured context.
pub(crate) fn runtime_provider_context_digest(
    request: &crate::simulator::ContinuousAgentRequestContextV1,
) -> String {
    let mut logical_request = request.clone();
    logical_request.transport_attempt = 1;
    crate::simulator::h_v1("oasis7.cognition.context.v1", &logical_request).to_string()
}

const COGNITION_LEASE_RESOURCE: &str = "cognition_units";

/// Construct the immutable economy request from the same logical provider
/// identity used by Runtime cognition dispatch. The request key deliberately
/// excludes transport attempt so retries reuse one reserved lease.
pub(super) fn reserve_provider_cognition_lease(
    world: &mut RuntimeWorld,
    context: &cognition_context::ProviderContextState,
) -> Result<crate::runtime::CognitionLeaseV1, String> {
    let request = provider_cognition_lease_request(world, context, false)?;
    world
        .reserve_cognition_lease(request)
        .map_err(|error| format!("cognition lease admission rejected: {error:?}"))
}

pub(in crate::viewer::runtime_live) fn provider_cognition_lease_request(
    world: &RuntimeWorld,
    context: &cognition_context::ProviderContextState,
    canonical_service: bool,
) -> Result<crate::runtime::CognitionLeaseRequestV1, String> {
    let request = &context.request_context;
    let runtime_binding = if canonical_service {
        request.runtime_binding.clone()
    } else {
        world
            .current_cognition_runtime_binding()
            .map_err(|error| format!("provider cognition Runtime binding unavailable: {error:?}"))?
    };
    if runtime_binding != request.runtime_binding {
        return Err("provider cognition Runtime binding changed before lease reserve".to_string());
    }
    let payer_id = if canonical_service {
        payer_support::provider_payer_id(request)?
    } else {
        payer_support::runtime_authorized_provider_payer_id(world, request)?
    };
    let invocation_key = request.provider_invocation_key().to_string();
    let quote = crate::runtime::CognitionLeaseQuoteV1::new(
        format!("cognition-quote:{invocation_key}"),
        COGNITION_LEASE_RESOURCE,
        1,
    )
    .with_authority(
        payer_id.clone(),
        crate::runtime::COGNITION_RESOURCE_VERSION_V1,
        "provider_cognition",
        "agent_turn",
        crate::runtime::COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION,
        request.capability_invocation_context_digest.to_string(),
        runtime_binding.base_world_hash.to_string(),
    );
    Ok(crate::runtime::CognitionLeaseRequestV1::new(
        invocation_key,
        payer_id,
        request.agent_subject.clone(),
        request.agent_session_id.clone(),
        request.agent_turn_id.clone(),
        request.decision_request_id.clone(),
        request.request_digest.to_string(),
        quote,
    ))
}

impl RuntimeLlmSidecar {
    fn provider_failure_at_authority(
        &mut self,
        world: &mut RuntimeWorld,
        context: &cognition_context::ProviderContextState,
        reason: &str,
    ) -> Result<(), String> {
        if self.provider_service_config.is_none() {
            if self.provider_service_required {
                return Err("chain-linked provider failure requires canonical service".into());
            }
            return runtime_provider_failure(world, context, reason);
        }
        self.provider_scheduler_operation(
            &context.request_context,
            &format!("failure:{}", context.request_context.transport_attempt),
            crate::world_service::wire::SchedulerOperationV1::ProviderFailure {
                request: crate::simulator::agent_api_compat::request_context_to_api(
                    &context.request_context,
                ),
                reason: reason.into(),
            },
        )
        .map(|_| ())
    }
    pub(in crate::viewer::runtime_live) fn release_provider_lease_at_authority(
        &mut self,
        world: &mut RuntimeWorld,
        request: &crate::simulator::ContinuousAgentRequestContextV1,
        lease: &crate::runtime::CognitionLeaseV1,
    ) -> Result<(), String> {
        if self.provider_service_config.is_none() {
            if self.provider_service_required {
                return Err("chain-linked lease release requires canonical service".into());
            }
            return world
                .release_cognition_lease(&lease.lease_id)
                .map(|_| ())
                .map_err(|error| format!("{error:?}"));
        }
        let receipt = self.provider_scheduler_operation(
            request,
            "release",
            crate::world_service::wire::SchedulerOperationV1::ReleaseLease {
                lease_id: lease.lease_id.clone(),
            },
        )?;
        Self::validate_service_release_receipt(request, lease, receipt)
    }

    pub(in crate::viewer::runtime_live) fn settle_provider_lease_at_authority(
        &mut self,
        world: &mut RuntimeWorld,
        request: &crate::simulator::ContinuousAgentRequestContextV1,
        lease: &crate::runtime::CognitionLeaseV1,
    ) -> Result<(), String> {
        if self.provider_service_config.is_none() {
            if self.provider_service_required {
                return Err("chain-linked lease settlement requires canonical service".into());
            }
            return world
                .settle_cognition_lease(&lease.lease_id, lease.reserved_amount)
                .map(|_| ())
                .map_err(|error| format!("{error:?}"));
        }
        self.provider_scheduler_operation(
            request,
            "settle",
            crate::world_service::wire::SchedulerOperationV1::SettleLease {
                lease_id: lease.lease_id.clone(),
                consumed_amount: lease.reserved_amount,
            },
        )
        .map(|_| ())
    }

    pub(in crate::viewer::runtime_live) fn reserve_provider_lease_at_authority(
        &mut self,
        world: &mut RuntimeWorld,
        context: &cognition_context::ProviderContextState,
    ) -> Result<crate::runtime::CognitionLeaseV1, String> {
        if self.provider_service_config.is_none() {
            if self.provider_service_required {
                return Err(
                    "chain-linked cognition lease requires canonical service configuration".into(),
                );
            }
            return reserve_provider_cognition_lease(world, context);
        }
        let request = provider_cognition_lease_request(world, context, true)?;
        let receipt = self.provider_scheduler_operation(
            &context.request_context,
            "reserve",
            crate::world_service::wire::SchedulerOperationV1::ReserveLease(request),
        )?;
        serde_json::from_value(receipt)
            .map_err(|error| format!("canonical cognition lease receipt invalid: {error}"))
    }

    pub(in crate::viewer::runtime_live) fn provider_prefix_at_authority(
        &mut self,
        world: &mut RuntimeWorld,
        context: &cognition_context::ProviderContextState,
    ) -> Result<(), String> {
        if self.provider_service_config.is_none() {
            if self.provider_service_required {
                return Err(
                    "chain-linked cognition prefix requires canonical service configuration".into(),
                );
            }
            return runtime_provider_prefix(world, context);
        }
        self.provider_scheduler_operation(
            &context.request_context,
            &format!("prefix:{}", context.request_context.transport_attempt),
            crate::world_service::wire::SchedulerOperationV1::ProviderPrefix {
                request: crate::simulator::agent_api_compat::request_context_to_api(
                    &context.request_context,
                ),
                context_digest: runtime_provider_context_digest(&context.request_context),
            },
        )
        .map(|_| ())
    }

    pub(super) fn provider_scheduler_operation(
        &mut self,
        request: &crate::simulator::ContinuousAgentRequestContextV1,
        phase: &str,
        operation: crate::world_service::wire::SchedulerOperationV1,
    ) -> Result<serde_json::Value, String> {
        self.provider_scheduler_operation_with_resume_context(request, phase, operation, None)
    }

    pub(super) fn provider_scheduler_operation_with_resume_context(
        &mut self,
        request: &crate::simulator::ContinuousAgentRequestContextV1,
        phase: &str,
        operation: crate::world_service::wire::SchedulerOperationV1,
        prepared: Option<(
            cognition_context::ProviderContextState,
            crate::simulator::ContinuationCurrentContextV1,
        )>,
    ) -> Result<serde_json::Value, String> {
        use crate::world_service::client::{RemoteWorldServiceClient, WorldServicePort};
        use oasis7_client_api::world_service::*;
        let config = self
            .provider_service_config
            .clone()
            .ok_or("canonical provider service missing")?;
        let (pending, existed) =
            self.prepare_service_scheduler_checkpoint(request, phase, operation, prepared)?;
        let client = RemoteWorldServiceClient::new(config)
            .map_err(|error| error.to_string())?
            .with_query_state(self.provider_service_query_state.clone());
        let lookup = if existed {
            let response = client
                .lookup(
                    LookupIntentRequest {
                        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                        key: pending.correlation.key.clone(),
                    },
                    pending.payload.clone(),
                )
                .map_err(|error| error.to_string())?;
            // The checkpoint is written before its first Submit. A verified
            // absence may therefore mean that transport never started. Only
            // that exact absence permits replaying the immutable original;
            // Pending/Received retain their existing execution without Submit.
            response
                .validate(&pending.correlation)
                .map_err(|error| error.to_string())?;
            Some(response)
        } else {
            None
        };
        let response = match lookup {
            Some(response) if !matches!(&response.outcome, IntentOutcome::Unknown) => response,
            _ => match client
                .submit(SubmitIntentRequest {
                    contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                    correlation: pending.correlation.clone(),
                    deadline_unix_ms: None,
                    signed_payload: pending.payload.clone(),
                })
                .map_err(|error| error.to_string())?
            {
                SubmitObservation::Response(response) => *response,
                SubmitObservation::OutcomeUnknown(_) => {
                    return Err(
                        "canonical scheduler outcome unknown; original request retained".into(),
                    );
                }
            },
        };
        response
            .validate(&pending.correlation)
            .map_err(|error| error.to_string())?;
        match response.outcome {
            IntentOutcome::Committed { receipt, .. } => Ok(receipt),
            state @ (IntentOutcome::Received { .. } | IntentOutcome::Pending) => {
                Err(format!("canonical scheduler remains unresolved: {state:?}"))
            }
            IntentOutcome::Rejected { reason } => {
                Err(format!("canonical scheduler rejected: {reason:?}"))
            }
            other => Err(format!(
                "canonical scheduler cannot complete original request: {other:?}"
            )),
        }
    }
}

impl RuntimeLlmDecision {
    pub(super) fn from_error(world: &RuntimeWorld, message: String) -> Self {
        let agent_id = world
            .state()
            .agents
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| "runtime-agent-0".to_string());
        Self::from_agent_error(world, agent_id, message)
    }

    fn from_agent_error(world: &RuntimeWorld, agent_id: String, message: String) -> Self {
        let trace = AgentDecisionTrace {
            agent_id: agent_id.clone(),
            time: world.state().time,
            decision: AgentDecision::Wait,
            llm_input: None,
            llm_output: None,
            llm_error: Some(message),
            parse_error: None,
            llm_diagnostics: None,
            llm_effect_intents: Vec::new(),
            llm_effect_receipts: Vec::new(),
            llm_step_trace: Vec::new(),
            llm_prompt_section_trace: Vec::new(),
            llm_chat_messages: Vec::new(),
        };
        Self {
            agent_id,
            decision: AgentDecision::Wait,
            decision_trace: Some(trace),
            cognition: None,
            memory_write_intents: Vec::new(),
            continuation_admitted: false,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl RuntimeLlmSidecar {
    pub(in crate::viewer::runtime_live) fn next_async_provider_decision(
        &mut self,
        world: &mut RuntimeWorld,
        kernel: &mut WorldKernel,
        world_id: &str,
    ) -> Option<RuntimeLlmDecision> {
        // Retry notifications that survived a previous chain-link tick or
        // process restart before admitting another provider turn.
        self.flush_pending_provider_world_events();
        let completed = {
            let Some(runner) = self
                .runner
                .as_mut()
                .and_then(RuntimeDecisionRunner::async_runner_mut)
            else {
                return Some(RuntimeLlmDecision::from_error(
                    world,
                    "provider runner not initialized".to_string(),
                ));
            };
            match runner.poll_completed() {
                Ok(outcomes) => outcomes,
                Err(error) => {
                    return Some(RuntimeLlmDecision::from_error(
                        world,
                        format!("async provider runner poll failed: {error}"),
                    ));
                }
            }
        };
        if let Some(runner) = self
            .runner
            .as_mut()
            .and_then(RuntimeDecisionRunner::async_runner_mut)
        {
            // `poll_completed` also records outcomes for compatibility users;
            // this production adapter consumes them into its own decision
            // queue so an outcome cannot accumulate across world ticks.
            let _ = runner.take_completed();
        }
        for outcome in completed {
            if outcome
                .prepared_request_context
                .as_ref()
                .is_some_and(|request| {
                    self.provider_terminal_matches_request(outcome.agent_id.as_str(), request)
                })
            {
                self.record_late_provider_response(&outcome);
                tracing::warn!(
                    agent_id = outcome.agent_id,
                    "discarded late provider response after terminal feedback"
                );
                continue;
            }
            let decision = self.provider_decision_from_async_outcome(world, kernel, outcome);
            self.provider_completed_decisions.push_back(decision);
        }
        if !self.provider_completed_decisions.is_empty() {
            self.persist_provider_lineage_best_effort();
        }
        while let Some(decision) = self.provider_completed_decisions.pop_front() {
            if self.provider_decision_is_terminalized(&decision) {
                // A terminal marker is authoritative even when a crash left
                // the same response in the completed queue. Drop only the
                // exact old identity; a later request for this Agent remains
                // eligible because it carries a different turn/request pair.
                self.persist_provider_lineage_best_effort();
                continue;
            }
            self.provider_held_decisions
                .insert(decision.agent_id.clone(), decision.clone());
            self.persist_provider_lineage_best_effort();
            return Some(decision);
        }

        // Legacy compatibility actors may enter before production recovery is
        // loaded. Let their existing completions drain, but do not reserve a
        // Runtime lease for a new production request until the authority
        // projection has been restored. The Harness keeps the final strict
        // start-turn guard; this earlier check avoids a provisional lease,
        // failure feedback, and release cycle on every idle candidate.
        if self
            .runner
            .as_ref()
            .and_then(RuntimeDecisionRunner::async_runner)
            .is_some_and(|runner| {
                runner.needs_runtime_feedback_recovery()
                    && !runner.runtime_feedback_history_loaded()
            })
        {
            return None;
        }

        if let Err(message) = self.prepare_provider_request_contexts(world, kernel, world_id) {
            return Some(RuntimeLlmDecision::from_error(world, message));
        }
        let now = world.state().time;
        let candidates = self.provider_agent_ids.iter().cloned().collect::<Vec<_>>();
        for agent_id in candidates {
            if self.provider_transport_exhausted.contains(&agent_id)
                || self.provider_recovery_pending.contains_key(&agent_id)
                || self
                    .provider_continuation_recovery_pending
                    .contains_key(&agent_id)
                || self.provider_active_turns.contains_key(&agent_id)
                || self
                    .provider_wait_until
                    .get(&agent_id)
                    .is_some_and(|wait_until| *wait_until > now)
            {
                continue;
            }
            let in_flight = self
                .runner
                .as_ref()
                .and_then(RuntimeDecisionRunner::async_runner)
                .is_some_and(|runner| runner.provider_is_still_in_flight(agent_id.as_str()));
            if in_flight {
                continue;
            }
            let Some(context) = self.provider_contexts.get(&agent_id).cloned() else {
                continue;
            };
            let recovery_blocked = self
                .runner
                .as_ref()
                .and_then(RuntimeDecisionRunner::async_runner)
                .is_some_and(|runner| {
                    super::super::llm_sidecar_feedback_recovery::feedback_history_blocks_dispatch(
                        runner,
                        agent_id.as_str(),
                        context.request_context.agent_session_id.as_str(),
                    )
                });
            if recovery_blocked {
                continue;
            }
            if let Some(existing_lease) = self.provider_cognition_lease(agent_id.as_str())
                && let Err(error) = self.validate_provider_cognition_lease_for_request(
                    world,
                    agent_id.as_str(),
                    &context.request_context,
                    &existing_lease,
                    "dispatch",
                )
            {
                self.fence_provider_cognition_lease(agent_id.as_str(), &context, error.clone());
                return Some(RuntimeLlmDecision::from_agent_error(world, agent_id, error));
            }
            if let Err(error) = self.ensure_canonical_agent_durable_admission() {
                return Some(RuntimeLlmDecision::from_agent_error(world, agent_id, error));
            }
            let observation = match kernel.observe(agent_id.as_str()) {
                Ok(observation) => observation,
                Err(error) => {
                    return Some(RuntimeLlmDecision::from_agent_error(
                        world,
                        agent_id,
                        format!("provider observation failed: {error:?}"),
                    ));
                }
            };
            let cognition_lease = match self.reserve_provider_lease_at_authority(world, &context) {
                Ok(lease) => lease,
                Err(error) => {
                    return Some(RuntimeLlmDecision::from_agent_error(world, agent_id, error));
                }
            };
            self.bind_provider_cognition_lease(agent_id.clone(), cognition_lease.clone());
            if let Err(error) = self.provider_prefix_at_authority(world, &context) {
                let release_error = self
                    .release_provider_lease_before_io_or_fence(
                        world,
                        agent_id.as_str(),
                        &context,
                        &cognition_lease,
                    )
                    .err()
                    .map(|error| format!("; {error}"));
                return Some(RuntimeLlmDecision::from_agent_error(
                    world,
                    agent_id,
                    format!(
                        "Runtime cognition prefix rejected provider I/O: {error}{}",
                        release_error.unwrap_or_default()
                    ),
                ));
            }
            // The Runtime prefix above is durable evidence that this exact
            // logical request was admitted.  Persist the matching sidecar
            // active marker before handing the request to the actor: a crash
            // after actor start but before its outcome is observed must fence
            // the identity on restart instead of redispatching it.
            self.provider_active_turns
                .insert(agent_id.clone(), context.clone());
            if let Err(error) = self.persist_provider_lineage() {
                self.provider_active_turns.remove(agent_id.as_str());
                let release_error = self
                    .release_provider_lease_before_io_or_fence(
                        world,
                        agent_id.as_str(),
                        &context,
                        &cognition_lease,
                    )
                    .err()
                    .map(|error| format!("; {error}"));
                let _ = self.provider_failure_at_authority(world, &context, "persistence_failure");
                return Some(RuntimeLlmDecision::from_agent_error(
                    world,
                    agent_id,
                    format!(
                        "provider dispatch marker persistence failed: {error}{}",
                        release_error.unwrap_or_default()
                    ),
                ));
            }
            let Some(runner) = self
                .runner
                .as_mut()
                .and_then(RuntimeDecisionRunner::async_runner_mut)
            else {
                let release_error = self
                    .release_provider_lease_before_io_or_fence(
                        world,
                        agent_id.as_str(),
                        &context,
                        &cognition_lease,
                    )
                    .err()
                    .map(|error| format!("; {error}"))
                    .unwrap_or_default();
                let _ = self.provider_failure_at_authority(world, &context, "provider_failure");
                return Some(RuntimeLlmDecision::from_error(
                    world,
                    format!(
                        "provider runner disappeared while starting an async turn{release_error}"
                    ),
                ));
            };
            runner.sync_logical_tick(world.state().time);
            let start_result = runner.start_turn_with_request_context_and_observation_and_lease(
                agent_id.as_str(),
                observation,
                context.turn_context.clone(),
                context.request_context.clone(),
                cognition_lease.clone(),
            );
            if let Err(error) = start_result {
                self.provider_active_turns.remove(agent_id.as_str());
                let release_error = self
                    .release_provider_lease_before_io_or_fence(
                        world,
                        agent_id.as_str(),
                        &context,
                        &cognition_lease,
                    )
                    .err()
                    .map(|error| format!("; {error}"));
                // If this cleanup cannot be persisted, retaining the durable
                // marker is the safe outcome: restart recovery will fence the
                // identity rather than risk a duplicate provider call.
                self.persist_provider_lineage_best_effort();
                let _ = self.provider_failure_at_authority(world, &context, "provider_failure");
                return Some(RuntimeLlmDecision::from_agent_error(
                    world,
                    agent_id,
                    format!(
                        "async provider turn start failed: {error}{}",
                        release_error.unwrap_or_default()
                    ),
                ));
            }
            // The actor now owns the provider call. Returning without a
            // decision lets the caller poll again without admitting another
            // turn; the caller must preserve this request's Runtime base
            // until a later poll surfaces the completed outcome.
            return None;
        }
        None
    }

    pub(super) fn provider_decision_from_async_outcome(
        &mut self,
        world: &mut RuntimeWorld,
        kernel: &mut WorldKernel,
        outcome: AsyncAgentTurnOutcome,
    ) -> RuntimeLlmDecision {
        let agent_id = outcome.agent_id.clone();
        let cognition_lease = outcome
            .cognition_lease
            .clone()
            .or_else(|| self.provider_cognition_lease(agent_id.as_str()));
        if let Some(lease) = cognition_lease.as_ref() {
            self.bind_provider_cognition_lease(agent_id.clone(), lease.clone());
        }
        let context = outcome
            .prepared_context
            .clone()
            .zip(outcome.prepared_request_context.clone())
            .map(
                |(turn_context, request_context)| cognition_context::ProviderContextState {
                    turn_context,
                    request_context,
                },
            )
            .or_else(|| self.provider_contexts.get(&agent_id).cloned());
        if let Some(context) = context.as_ref() {
            self.provider_contexts
                .insert(agent_id.clone(), context.clone());
        }
        let cognition = context
            .clone()
            .zip(outcome.prepared_response_context.clone())
            .map(|(request, response)| RuntimeProviderActionContext {
                request,
                response,
                cognition_lease: cognition_lease.clone(),
                memory_write_intents: outcome.memory_write_intents.clone(),
            });
        if let Some(context) = context.as_ref() {
            let Some(lease) = cognition_lease.as_ref() else {
                let error = format!(
                    "provider cognition lease missing for {}",
                    context.request_context.agent_subject
                );
                self.fence_provider_cognition_lease(agent_id.as_str(), context, error.clone());
                return RuntimeLlmDecision::from_agent_error(world, agent_id, error);
            };
            if let Err(error) = self.validate_provider_cognition_lease_for_request(
                world,
                agent_id.as_str(),
                &context.request_context,
                lease,
                "outcome",
            ) {
                self.fence_provider_cognition_lease(agent_id.as_str(), context, error.clone());
                return RuntimeLlmDecision::from_agent_error(world, agent_id, error);
            }
        }
        let decision = outcome.decision.unwrap_or(AgentDecision::Wait);
        let mut decision_trace = outcome.decision_trace.or_else(|| {
            (outcome.lifecycle == AsyncTurnLifecycle::Failed).then(|| AgentDecisionTrace {
                agent_id: agent_id.clone(),
                time: world.state().time,
                decision: AgentDecision::Wait,
                llm_input: None,
                llm_output: None,
                llm_error: Some(format!(
                    "async provider actor failed: {:?}",
                    outcome.feedback
                )),
                parse_error: None,
                llm_diagnostics: None,
                llm_effect_intents: Vec::new(),
                llm_effect_receipts: Vec::new(),
                llm_step_trace: Vec::new(),
                llm_prompt_section_trace: Vec::new(),
                llm_chat_messages: Vec::new(),
            })
        });
        if cognition.is_none()
            && decision_trace
                .as_ref()
                .is_some_and(is_budget_exhausted_wait)
            && context.is_some()
        {
            // The actor's budget normalizer intentionally emits a no-effect
            // Wait, but custom/provider lanes may not produce a response
            // artifact for that denial.  Keep the request/lease in the
            // sidecar recovery maps and route it through the same terminal
            // cleanup pass as an exhausted transport; otherwise the control
            // plane sees a bare Wait and leaves the awaiting Runtime turn
            // occupied forever.
            self.mark_provider_transport_exhausted(agent_id.clone());
        }
        let mut continuation_admitted = false;
        if let Some(cognition) = cognition.as_ref() {
            if decision_trace.as_ref().is_none_or(|trace| {
                trace.parse_error.is_none()
                    && (trace.llm_error.is_none()
                        || is_trace_only_overflow(trace)
                        || is_budget_exhausted_wait(trace))
            }) {
                self.provider_active_turns
                    .insert(agent_id.clone(), cognition.request.clone());
                match &decision {
                    AgentDecision::Wait => {
                        continuation_admitted =
                            match self.admit_provider_wait_continuation(world, kernel, cognition) {
                                Ok(()) => true,
                                Err(error) => {
                                    tracing::warn!(
                                        agent_id = agent_id.as_str(),
                                        error = error.as_str(),
                                        "provider Wait continuation admission failed"
                                    );
                                    if !self.provider_service_required {
                                        let _ = self.provider_failure_at_authority(
                                            world,
                                            &cognition.request,
                                            "continuation_admission_failed",
                                        );
                                    }
                                    let mut trace = decision_trace.clone().unwrap_or_else(|| {
                                        AgentDecisionTrace {
                                            agent_id: agent_id.clone(),
                                            time: world.state().time,
                                            decision: AgentDecision::Wait,
                                            llm_input: None,
                                            llm_output: None,
                                            llm_error: None,
                                            parse_error: None,
                                            llm_diagnostics: None,
                                            llm_effect_intents: Vec::new(),
                                            llm_effect_receipts: Vec::new(),
                                            llm_step_trace: Vec::new(),
                                            llm_prompt_section_trace: Vec::new(),
                                            llm_chat_messages: Vec::new(),
                                        }
                                    });
                                    trace.llm_error = Some(error);
                                    decision_trace = Some(trace);
                                    false
                                }
                            };
                    }
                    AgentDecision::WaitTicks(ticks) => {
                        if self.provider_service_required {
                            match self.admit_provider_wait_continuation(world, kernel, cognition) {
                                Ok(()) => continuation_admitted = true,
                                Err(error) => {
                                    let mut trace = AgentDecisionTrace {
                                        agent_id: agent_id.clone(),
                                        time: world.state().time,
                                        decision: decision.clone(),
                                        llm_input: None,
                                        llm_output: None,
                                        llm_error: Some(error),
                                        parse_error: None,
                                        llm_diagnostics: None,
                                        llm_effect_intents: Vec::new(),
                                        llm_effect_receipts: Vec::new(),
                                        llm_step_trace: Vec::new(),
                                        llm_prompt_section_trace: Vec::new(),
                                        llm_chat_messages: Vec::new(),
                                    };
                                    if let Some(existing) = decision_trace.take() {
                                        trace = existing;
                                        trace.llm_error = Some(
                                            "canonical timed Wait admission remains pending".into(),
                                        );
                                    }
                                    decision_trace = Some(trace);
                                }
                            }
                        } else {
                            self.schedule_provider_wait(
                                agent_id.as_str(),
                                world.state().time,
                                *ticks,
                            );
                        }
                    }
                    AgentDecision::Act(_)
                    | AgentDecision::Query(_)
                    | AgentDecision::ModuleCommand { .. } => {}
                }
            }
        } else if decision_trace
            .as_ref()
            .is_some_and(provider_trace_retryable)
        {
            if let Some(context) = context {
                if context.request_context.transport_attempt < MAX_PROVIDER_TRANSPORT_ATTEMPTS {
                    let mut retry_context = context.request_context.clone();
                    retry_context.transport_attempt =
                        retry_context.transport_attempt.saturating_add(1);
                    let retry_provider_context = cognition_context::ProviderContextState {
                        request_context: retry_context.clone(),
                        turn_context: context.turn_context.clone(),
                    };
                    if let Err(error) =
                        self.provider_prefix_at_authority(world, &retry_provider_context)
                    {
                        tracing::warn!(
                            agent_id,
                            error,
                            "Runtime cognition retry dispatch rejected"
                        );
                        let _ = self.provider_failure_at_authority(
                            world,
                            &context,
                            "persistence_failure",
                        );
                        self.mark_provider_transport_exhausted(agent_id.clone());
                        self.persist_provider_lineage_best_effort();
                        return RuntimeLlmDecision {
                            agent_id,
                            decision,
                            decision_trace,
                            cognition,
                            memory_write_intents: outcome.memory_write_intents,
                            continuation_admitted,
                        };
                    }
                    let retry_result = kernel
                        .observe(agent_id.as_str())
                        .map_err(|error| format!("provider retry observation failed: {error:?}"))
                        .and_then(|observation| {
                            let Some(runner) = self
                                .runner
                                .as_mut()
                                .and_then(RuntimeDecisionRunner::async_runner_mut)
                            else {
                                return Err(
                                    "provider runner disappeared while retrying an async turn"
                                        .to_string(),
                                );
                            };
                            runner
                                .retry_awaiting_turn_with_request_context_and_observation(
                                    agent_id.as_str(),
                                    observation,
                                    context.turn_context.clone(),
                                    context.request_context.clone(),
                                )
                                .map(|_| ())
                                .map_err(|error| error.to_string())
                        });
                    if let Err(error) = retry_result {
                        tracing::warn!(
                            agent_id,
                            error,
                            "async provider transport retry could not be dispatched"
                        );
                        // Runtime has already admitted transport attempt 2.
                        // Close that exact attempt when the actor cannot
                        // accept the retry; failing the stale attempt-1
                        // context leaves the durable Runtime turn open.
                        let retry_runtime_context = cognition_context::ProviderContextState {
                            turn_context: context.turn_context.clone(),
                            request_context: retry_context,
                        };
                        let _ = self.provider_failure_at_authority(
                            world,
                            &retry_runtime_context,
                            "failed_provider",
                        );
                        self.mark_provider_transport_exhausted(agent_id.clone());
                    }
                } else {
                    // Keep the last response correlated long enough for the
                    // Viewer control plane to emit one typed terminal
                    // `failed_provider` disposition. Without this marker the
                    // context remains reusable and every later world tick can
                    // redispatch the same exhausted transport attempt.
                    self.mark_provider_transport_exhausted(agent_id.clone());
                    let _ = self.provider_failure_at_authority(world, &context, "failed_provider");
                }
            }
        } else if let Some(context) = context {
            // A provider/actor failure that is not retryable must be closed
            // durably before the control plane observes the typed error.
            let _ = self.provider_failure_at_authority(world, &context, "provider_failure");
        }
        self.persist_provider_lineage_best_effort();
        RuntimeLlmDecision {
            agent_id,
            decision,
            decision_trace,
            cognition,
            memory_write_intents: outcome.memory_write_intents,
            continuation_admitted,
        }
    }
}

fn cognition_event_matches(
    world: &RuntimeWorld,
    kind: &str,
    request: &crate::simulator::ContinuousAgentRequestContextV1,
) -> bool {
    let expected_context_digest =
        (kind == "ContextCaptured").then(|| runtime_provider_context_digest(request));
    world
        .cognition()
        .get("cognition_journal")
        .and_then(|journal| journal.get("events"))
        .and_then(serde_json::Value::as_array)
        .is_some_and(|events| {
            events.iter().any(|event| {
                event.get("event_kind").and_then(serde_json::Value::as_str) == Some(kind)
                    && event.get("agent_id").and_then(serde_json::Value::as_str)
                        == Some(request.agent_subject.as_str())
                    && event
                        .get("agent_session_id")
                        .and_then(serde_json::Value::as_str)
                        == Some(request.agent_session_id.as_str())
                    && event
                        .get("agent_turn_id")
                        .and_then(serde_json::Value::as_str)
                        == Some(request.agent_turn_id.as_str())
                    && event
                        .get("decision_request_id")
                        .and_then(serde_json::Value::as_str)
                        == Some(request.decision_request_id.as_str())
                    && event
                        .get("request_digest")
                        .and_then(serde_json::Value::as_str)
                        == Some(request.request_digest.to_string().as_str())
                    && expected_context_digest.as_deref().is_none_or(|expected| {
                        event
                            .get("context_digest")
                            .and_then(serde_json::Value::as_str)
                            == Some(expected)
                    })
            })
        })
}

pub(super) fn runtime_provider_prefix(
    world: &mut RuntimeWorld,
    context: &cognition_context::ProviderContextState,
) -> Result<(), String> {
    let request = &context.request_context;
    crate::world_service::game_task::validate_cognition_fence_staged(
        world,
        &request.agent_subject,
        None,
        "",
        0,
        [],
    )?;
    let request_digest = request.request_digest.to_string();
    if !cognition_event_matches(world, "TurnStarted", request) {
        world
            .start_cognition_turn(
                request.agent_subject.as_str(),
                request.agent_session_id.as_str(),
                request.agent_turn_id.as_str(),
                request.decision_request_id.as_str(),
                request_digest.as_str(),
            )
            .map_err(|error| format!("TurnStarted failed: {error:?}"))?;
    }
    if !cognition_event_matches(world, "ContextCaptured", request) {
        let context_digest = runtime_provider_context_digest(request);
        world
            .capture_cognition_context(
                request.agent_subject.as_str(),
                request.agent_session_id.as_str(),
                request.agent_turn_id.as_str(),
                request.decision_request_id.as_str(),
                request_digest.as_str(),
                context_digest.as_str(),
            )
            .map_err(|error| format!("ContextCaptured failed: {error:?}"))?;
    }
    runtime_provider_dispatch(world, request)
}

pub(super) fn runtime_provider_dispatch(
    world: &mut RuntimeWorld,
    request: &crate::simulator::ContinuousAgentRequestContextV1,
) -> Result<(), String> {
    crate::world_service::game_task::validate_cognition_fence_staged(
        world,
        &request.agent_subject,
        None,
        "",
        0,
        [],
    )?;
    world
        .dispatch_cognition_request(
            request.agent_subject.as_str(),
            request.agent_session_id.as_str(),
            request.agent_turn_id.as_str(),
            request.decision_request_id.as_str(),
            request.request_digest.to_string().as_str(),
            request.provider_invocation_key().to_string().as_str(),
            request.retry_seq,
            request.transport_attempt,
        )
        .map_err(|error| format!("RequestDispatched failed: {error:?}"))
}

pub(super) fn runtime_provider_failure(
    world: &mut RuntimeWorld,
    context: &cognition_context::ProviderContextState,
    reason: &str,
) -> Result<(), String> {
    let request = &context.request_context;
    world
        .fail_cognition_turn(
            request.agent_subject.as_str(),
            request.agent_session_id.as_str(),
            request.agent_turn_id.as_str(),
            request.decision_request_id.as_str(),
            request.request_digest.to_string().as_str(),
            reason,
            request.retry_seq,
            request.transport_attempt,
        )
        .map_err(|error| format!("CognitionTurnFailed failed: {error:?}"))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "llm_sidecar_async_feedback_recovery_tests.rs"]
mod feedback_recovery_admission_tests;
