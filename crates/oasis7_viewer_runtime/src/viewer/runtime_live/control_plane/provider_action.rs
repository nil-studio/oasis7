#[path = "provider_action_feedback_diagnostics.rs"]
mod feedback_diagnostics;
use feedback_diagnostics::{
    canonical_feedback_reason, stale_replan_exhausted_trace, wake_handoff_error_trace,
};

use super::super::decision_trace::{is_budget_exhausted_wait, is_trace_only_overflow};
use super::*;
use crate::runtime::{RuntimeFeedbackProjectionV1, RuntimeFeedbackRequestV1};
use crate::simulator::AgentDecision;

#[cfg(test)]
#[path = "provider_action_service_tests.rs"]
mod canonical_service_fencing_tests;

#[path = "provider_action_service.rs"]
mod service_provider;

impl ViewerRuntimeLiveServer {
    /// Report a stale parent only if a bounded replacement was actually queued.
    pub(in crate::viewer::runtime_live) fn schedule_provider_stale_replan_disposition(
        &mut self,
        agent_id: &str,
        parent_turn_id: &str,
        parent_request_id: &str,
    ) -> (&'static str, bool) {
        if self.llm_sidecar.schedule_provider_stale_replan(
            agent_id,
            parent_turn_id,
            parent_request_id,
        ) {
            ("stale_base", true)
        } else {
            ("provider_stale_replan_exhausted", false)
        }
    }

    #[expect(
        clippy::result_large_err,
        reason = "Provider decision traces retain the stable typed transport error envelope"
    )]
    pub(in crate::viewer::runtime_live) fn enqueue_llm_action_from_sidecar(
        &mut self,
    ) -> Result<Option<AgentDecisionTrace>, AgentDecisionTrace> {
        self.configure_service_provider();
        // A chain-linked World is an observation projection. In particular,
        // recovery and request preparation below can settle leases and advance
        // wakes before a decision is returned. Fence the entire pass until the
        // canonical cognition service adapter owns those transitions.
        if self.chain_link_enabled() {
            if self.config.world_service.is_some() {
                return self.enqueue_service_provider_action();
            }
            return Err(wake_handoff_error_trace(
                "world_service",
                self.world.state().time,
                "canonical Agent cognition service is unavailable; provider turn remains pending"
                    .to_string(),
            ));
        }
        self.recover_checkpoint_terminal_feedback()
            .map_err(|error| {
                wake_handoff_error_trace("runtime-feedback", self.world.state().time, error)
            })?;
        self.drain_provider_feedback_outbox();
        // A failed wake retry fences only its own Agent. Keep the error for
        // an actionable no-decision result while allowing a healthy sibling
        // to use this same control pass.
        let wake_recovery_error = self
            .llm_sidecar
            .provider_wake_recovery_pending_agent()
            .zip(self.retry_provider_wake_recovery().err());
        let unresolved_wake_agent = wake_recovery_error
            .as_ref()
            .map(|(agent_id, _)| agent_id.as_str());
        if let Some(agent_id) = self.llm_sidecar.provider_stale_replan_exhausted_agent() {
            // Stale binding recovery owns the old lease. Run that cleanup
            // before terminalizing an exhausted replan; otherwise the early
            // return would leave a Reserved mirror and block durable recovery.
            let stale_replan_cleanup = self
                .llm_sidecar
                .release_binding_changed_provider_leases(&mut self.world)
                .err();
            let mut trace = stale_replan_exhausted_trace(&self.world, agent_id.as_str());
            if let Some(error) = stale_replan_cleanup {
                trace.llm_error = Some(format!(
                    "{}; stale provider lease cleanup remains pending: {error}",
                    trace.llm_error.take().unwrap_or_default()
                ));
                return Err(trace);
            }
            if let Err(error) = self.handoff_runtime_wake_for_agent(
                agent_id.as_str(),
                crate::runtime::ContinuationStatusV1::Rejected,
                "provider_stale_replan_exhausted",
            ) {
                trace.llm_error = Some(format!(
                    "{}; Runtime wake handoff failed: {error}",
                    trace.llm_error.take().unwrap_or_default()
                ));
            }
            return Err(trace);
        }
        if let Some(agent_id) = self
            .llm_sidecar
            .provider_transport_exhausted_agent_excluding(unresolved_wake_agent)
        {
            let lease = self.llm_sidecar.provider_cognition_lease(agent_id.as_str());
            return Err(self.finish_provider_transport_exhaustion(agent_id, None, lease));
        }
        if let Some((_, agent_id, _)) = self.llm_sidecar.pending_provider_action_for_recovery()
            && let Err(error) = self.retry_committed_provider_action()
        {
            return Err(wake_handoff_error_trace(
                agent_id.as_str(),
                self.world.state().time,
                format!("Runtime receipt finalization recovery remains pending: {error}"),
            ));
        }
        let decision = self.llm_sidecar.next_llm_decision(
            &mut self.world,
            &self.snapshot_config,
            self.config.world_id.as_str(),
        );
        // `next_llm_decision` may close a due provider Wait in the Runtime
        // outbox before returning a new decision. Drain that newly queued
        // terminal feedback in the same control pass so the provider sees
        // canonical `rejected/no_effect` rather than only the earlier
        // `pending` disposition.
        self.drain_provider_feedback_outbox();
        let Some(decision) = decision else {
            if let Some((agent_id, error)) = wake_recovery_error {
                return Err(wake_handoff_error_trace(
                    agent_id.as_str(),
                    self.world.state().time,
                    format!("Runtime wake recovery remains pending: {error}"),
                ));
            }
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(agent_id) = self.llm_sidecar.provider_wait_recovery_requires_attention() {
                return Err(wake_handoff_error_trace(
                    agent_id.as_str(),
                    self.world.state().time,
                    "provider Wait continuation recovery remains pending".to_string(),
                ));
            }
            return Ok(None);
        };
        let decision_trace = decision.decision_trace.clone();
        if let Some(agent_id) = self
            .llm_sidecar
            .provider_transport_exhausted_agent_excluding(unresolved_wake_agent)
        {
            let lease = decision
                .cognition
                .as_ref()
                .and_then(|cognition| cognition.cognition_lease.clone())
                .or_else(|| self.llm_sidecar.provider_cognition_lease(agent_id.as_str()));
            return Err(self.finish_provider_transport_exhaustion(agent_id, decision_trace, lease));
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(trace) = decision_trace.as_ref()
            && self
                .llm_sidecar
                .has_provider_wait_recovery(decision.agent_id.as_str())
        {
            // A failed continuation compensation owns the exact request
            // identity until its durable recovery pass succeeds. Do not
            // let the generic provider-error path release that identity.
            return Err(trace.clone());
        }
        if let Some(trace) = decision_trace.as_ref() {
            if trace.llm_error.is_some()
                && !is_trace_only_overflow(trace)
                && !is_budget_exhausted_wait(trace)
            {
                if !decision_trace_provider_error_retryable(trace).unwrap_or(false) {
                    self.settle_provider_cognition_lease_for_response_or_release(
                        decision.agent_id.as_str(),
                        decision
                            .cognition
                            .as_ref()
                            .and_then(|cognition| cognition.cognition_lease.clone()),
                        decision
                            .cognition
                            .as_ref()
                            .map(|cognition| &cognition.request.request_context),
                    )
                    .map_err(|error| {
                        wake_handoff_error_trace(
                            decision.agent_id.as_str(),
                            self.world.state().time,
                            error,
                        )
                    })?;
                    if let Some(feedback) = self
                        .llm_sidecar
                        .fail_provider_turn_with_feedback(
                            decision.agent_id.as_str(),
                            "failed",
                            trace
                                .llm_error
                                .clone()
                                .unwrap_or_else(|| "provider decision failed".to_string()),
                        )
                        .map_err(|error| {
                            wake_handoff_error_trace(
                                decision.agent_id.as_str(),
                                self.world.state().time,
                                error,
                            )
                        })?
                    {
                        self.deliver_provider_feedback_best_effort(feedback);
                    }
                    if let Err(error) = self.handoff_runtime_wake_for_agent(
                        decision.agent_id.as_str(),
                        crate::runtime::ContinuationStatusV1::Rejected,
                        "provider_decision_failed",
                    ) {
                        let mut terminal_trace = trace.clone();
                        terminal_trace.llm_error = Some(format!(
                            "{}; Runtime wake handoff failed: {error}",
                            terminal_trace.llm_error.take().unwrap_or_default()
                        ));
                        return Err(terminal_trace);
                    }
                }
                return Err(trace.clone());
            }
            if let Some(message) = trace.parse_error.as_ref() {
                self.settle_provider_cognition_lease_for_response_or_release(
                    decision.agent_id.as_str(),
                    decision
                        .cognition
                        .as_ref()
                        .and_then(|cognition| cognition.cognition_lease.clone()),
                    decision
                        .cognition
                        .as_ref()
                        .map(|cognition| &cognition.request.request_context),
                )
                .map_err(|error| {
                    wake_handoff_error_trace(
                        decision.agent_id.as_str(),
                        self.world.state().time,
                        error,
                    )
                })?;
                if let Some(feedback) = self
                    .llm_sidecar
                    .fail_provider_turn_with_feedback(
                        decision.agent_id.as_str(),
                        "rejected",
                        message.clone(),
                    )
                    .map_err(|error| {
                        wake_handoff_error_trace(
                            decision.agent_id.as_str(),
                            self.world.state().time,
                            error,
                        )
                    })?
                {
                    self.deliver_provider_feedback_best_effort(feedback);
                }
                self.enqueue_virtual_event(WorldEventKind::ActionRejected {
                    reason: SimulatorRejectReason::RuleDenied {
                        notes: vec![format!("llm_failed: {}", message)],
                    },
                });
                if let Err(error) = self.handoff_runtime_wake_for_agent(
                    decision.agent_id.as_str(),
                    crate::runtime::ContinuationStatusV1::Rejected,
                    "provider_response_parse_failed",
                ) {
                    let mut terminal_trace = trace.clone();
                    terminal_trace.llm_error =
                        Some(format!("Runtime wake handoff failed: {error}"));
                    return Err(terminal_trace);
                }
                return Ok(decision_trace);
            }
        }

        match decision.decision.clone() {
            AgentDecision::Act(action) => match simulator_action_to_runtime(&action, &self.world) {
                Some(runtime_action) => {
                    if let Some(cognition) = decision.cognition {
                        match self.commit_provider_runtime_action(
                            &runtime_action,
                            &cognition,
                            action,
                        ) {
                            Ok(()) => {}
                            Err(error) => {
                                if error.is_wake_handoff() {
                                    return Err(wake_handoff_error_trace(
                                        cognition.request.request_context.agent_subject.as_str(),
                                        self.world.state().time,
                                        error.reason(),
                                    ));
                                }
                                if error.is_post_commit() {
                                    return Err(wake_handoff_error_trace(
                                        cognition.request.request_context.agent_subject.as_str(),
                                        self.world.state().time,
                                        error.reason(),
                                    ));
                                }
                                self.settle_provider_cognition_lease_for_response_or_release(
                                    cognition.request.request_context.agent_subject.as_str(),
                                    cognition.cognition_lease.clone(),
                                    Some(&cognition.request.request_context),
                                )
                                .map_err(|release_error| {
                                    wake_handoff_error_trace(
                                        cognition.request.request_context.agent_subject.as_str(),
                                        self.world.state().time,
                                        release_error,
                                    )
                                })?;
                                let stale_base = error.is_stale_base();
                                let request = &cognition.request.request_context;
                                let (reason, stale_replan_queued) = if stale_base {
                                    let (reason, queued) = self
                                        .schedule_provider_stale_replan_disposition(
                                            request.agent_subject.as_str(),
                                            request.agent_turn_id.as_str(),
                                            request.decision_request_id.as_str(),
                                        );
                                    (reason.to_string(), queued)
                                } else {
                                    (error.reason(), false)
                                };
                                if let Some(feedback) = self
                                    .llm_sidecar
                                    .fail_provider_turn_with_feedback(
                                        cognition.request.request_context.agent_subject.as_str(),
                                        "rejected",
                                        reason.clone(),
                                    )
                                    .map_err(|error| {
                                        wake_handoff_error_trace(
                                            cognition
                                                .request
                                                .request_context
                                                .agent_subject
                                                .as_str(),
                                            self.world.state().time,
                                            error,
                                        )
                                    })?
                                {
                                    self.deliver_provider_feedback_best_effort(feedback);
                                }
                                self.enqueue_virtual_event(WorldEventKind::ActionRejected {
                                    reason: SimulatorRejectReason::RuleDenied {
                                        notes: vec![if stale_replan_queued {
                                            "stale_base".to_string()
                                        } else {
                                            reason
                                        }],
                                    },
                                });
                                let handoff_status = if stale_replan_queued {
                                    crate::runtime::ContinuationStatusV1::Invalidated
                                } else {
                                    crate::runtime::ContinuationStatusV1::Rejected
                                };
                                let handoff_reason = if stale_replan_queued {
                                    "provider_stale_base_replan"
                                } else {
                                    "provider_action_commit_rejected"
                                };
                                if let Err(error) = self.handoff_runtime_wake_for_agent(
                                    cognition.request.request_context.agent_subject.as_str(),
                                    handoff_status,
                                    handoff_reason,
                                ) {
                                    return Err(wake_handoff_error_trace(
                                        cognition.request.request_context.agent_subject.as_str(),
                                        self.world.state().time,
                                        error,
                                    ));
                                }
                            }
                        }
                    } else {
                        let action_id = self.world.submit_action(runtime_action);
                        self.llm_sidecar.track_action(
                            action_id,
                            decision.agent_id,
                            action.clone(),
                            None,
                        );
                    }
                }
                None => {
                    let reason = format!(
                        "runtime llm bridge cannot map action: {}",
                        simulator_action_label(&action)
                    );
                    self.settle_provider_cognition_lease_for_response_or_release(
                        decision.agent_id.as_str(),
                        decision
                            .cognition
                            .as_ref()
                            .and_then(|cognition| cognition.cognition_lease.clone()),
                        decision
                            .cognition
                            .as_ref()
                            .map(|cognition| &cognition.request.request_context),
                    )
                    .map_err(|error| {
                        wake_handoff_error_trace(
                            decision.agent_id.as_str(),
                            self.world.state().time,
                            error,
                        )
                    })?;
                    self.llm_sidecar
                        .fail_provider_cognition_turn(
                            &mut self.world,
                            decision.agent_id.as_str(),
                            "cognition_failed",
                        )
                        .map_err(|error| {
                            wake_handoff_error_trace(
                                decision.agent_id.as_str(),
                                self.world.state().time,
                                error,
                            )
                        })?;
                    if let Some(feedback) = self
                        .llm_sidecar
                        .fail_provider_turn_with_feedback(
                            decision.agent_id.as_str(),
                            "rejected",
                            reason.clone(),
                        )
                        .map_err(|error| {
                            wake_handoff_error_trace(
                                decision.agent_id.as_str(),
                                self.world.state().time,
                                error,
                            )
                        })?
                    {
                        self.deliver_provider_feedback_best_effort(feedback);
                    }
                    self.enqueue_virtual_event(WorldEventKind::ActionRejected {
                        reason: SimulatorRejectReason::RuleDenied {
                            notes: vec![reason],
                        },
                    });
                    self.handoff_runtime_wake_for_agent(
                        decision.agent_id.as_str(),
                        crate::runtime::ContinuationStatusV1::Rejected,
                        "provider_action_unmappable",
                    )
                    .map_err(|error| AgentDecisionTrace {
                        agent_id: decision.agent_id.clone(),
                        time: self.world.state().time,
                        decision: AgentDecision::Wait,
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
                    })?;
                }
            },
            AgentDecision::Wait | AgentDecision::WaitTicks(_) => {
                if let Some(cognition) = decision.cognition {
                    // A provider response means the fixed cognition unit was
                    // consumed even when the proposed outcome is Wait.  Keep
                    // release for no-I/O paths such as a native budget
                    // denial; successful Wait/WaitTicks must settle exactly
                    // once so repeated waits cannot run at zero net charge.
                    if decision_trace
                        .as_ref()
                        .is_some_and(is_budget_exhausted_wait)
                    {
                        self.release_provider_cognition_lease_for_request(
                            decision.agent_id.as_str(),
                            cognition.cognition_lease.clone(),
                            Some(&cognition.request.request_context),
                        )
                    } else {
                        let settlement = self.settle_provider_cognition_lease_for_request(
                            decision.agent_id.as_str(),
                            cognition.cognition_lease.clone(),
                            Some(&cognition.request.request_context),
                        );
                        if settlement.is_ok() {
                            self.llm_sidecar
                                .clear_provider_cognition_lease(decision.agent_id.as_str());
                        }
                        settlement
                    }
                    .map_err(|error| {
                        wake_handoff_error_trace(
                            decision.agent_id.as_str(),
                            self.world.state().time,
                            error,
                        )
                    })?;
                    let ticks = match &decision.decision {
                        AgentDecision::Wait => 1,
                        AgentDecision::WaitTicks(ticks) => (*ticks).max(1),
                        _ => unreachable!("wait branch is exhaustive"),
                    };
                    if !decision.continuation_admitted {
                        self.llm_sidecar.schedule_provider_wait(
                            decision.agent_id.as_str(),
                            self.world.state().time,
                            ticks,
                        );
                    }
                    let feedback = self.llm_sidecar.provider_feedback(
                        &cognition,
                        None,
                        "pending",
                        None,
                        None,
                        Some(if decision.continuation_admitted {
                            "continuation_admitted".to_string()
                        } else {
                            "retry_scheduled".to_string()
                        }),
                    );
                    self.deliver_provider_feedback_best_effort(feedback);
                    if decision.continuation_admitted {
                        // The native ProviderBacked path has already released
                        // the actor outcome while retaining the Harness chain;
                        // Runtime now owns the durable wake. No local timer or
                        // compatibility terminal handoff may close it.
                    } else if cognition.request.turn_context.continuation.is_some() {
                        // A Runtime-resumed request already consumed the
                        // selected wake and admitted its next continuation.
                        // Keep that continuation under the normal scheduler;
                        // local Viewer wait timers must not terminalize it.
                        self.llm_sidecar
                            .fail_provider_turn(decision.agent_id.as_str());
                    } else {
                        self.handoff_runtime_wake_for_agent(
                            decision.agent_id.as_str(),
                            crate::runtime::ContinuationStatusV1::Rejected,
                            "provider_wait_compatibility_terminal",
                        )
                        .map_err(|error| AgentDecisionTrace {
                            agent_id: decision.agent_id.clone(),
                            time: self.world.state().time,
                            decision: AgentDecision::Wait,
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
                        })?;
                    }
                }
            }
            AgentDecision::Query(_) => {
                if let Some(cognition) = decision.cognition {
                    self.settle_provider_cognition_lease_for_response_or_release(
                        decision.agent_id.as_str(),
                        cognition.cognition_lease.clone(),
                        Some(&cognition.request.request_context),
                    )
                    .map_err(|error| {
                        wake_handoff_error_trace(
                            decision.agent_id.as_str(),
                            self.world.state().time,
                            error,
                        )
                    })?;
                    self.llm_sidecar
                        .fail_provider_cognition_turn(
                            &mut self.world,
                            decision.agent_id.as_str(),
                            "cognition_failed",
                        )
                        .map_err(|error| {
                            wake_handoff_error_trace(
                                decision.agent_id.as_str(),
                                self.world.state().time,
                                error,
                            )
                        })?;
                    let feedback = self.llm_sidecar.provider_feedback(
                        &cognition,
                        None,
                        "rejected",
                        None,
                        None,
                        Some(
                            "provider query is not executable in the runtime live action lane"
                                .to_string(),
                        ),
                    );
                    self.llm_sidecar
                        .fail_provider_turn(decision.agent_id.as_str());
                    self.deliver_provider_feedback_best_effort(feedback);
                    self.handoff_runtime_wake_for_agent(
                        decision.agent_id.as_str(),
                        crate::runtime::ContinuationStatusV1::Rejected,
                        "provider_query_not_executable",
                    )
                    .map_err(|error| AgentDecisionTrace {
                        agent_id: decision.agent_id.clone(),
                        time: self.world.state().time,
                        decision: AgentDecision::Wait,
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
                    })?;
                }
            }
            AgentDecision::ModuleCommand { .. } => {
                if let Some(cognition) = decision.cognition {
                    self.settle_provider_cognition_lease_for_response_or_release(
                        decision.agent_id.as_str(),
                        cognition.cognition_lease.clone(),
                        Some(&cognition.request.request_context),
                    )
                    .map_err(|error| {
                        wake_handoff_error_trace(
                            decision.agent_id.as_str(),
                            self.world.state().time,
                            error,
                        )
                    })?;
                    self.llm_sidecar
                        .fail_provider_cognition_turn(
                            &mut self.world,
                            decision.agent_id.as_str(),
                            "cognition_failed",
                        )
                        .map_err(|error| {
                            wake_handoff_error_trace(
                                decision.agent_id.as_str(),
                                self.world.state().time,
                                error,
                            )
                        })?;
                    let feedback = self.llm_sidecar.provider_feedback(
                        &cognition,
                        None,
                        "rejected",
                        None,
                        None,
                        Some("module commands require the Runtime typed command lane".to_string()),
                    );
                    self.llm_sidecar
                        .fail_provider_turn(decision.agent_id.as_str());
                    self.deliver_provider_feedback_best_effort(feedback);
                    self.handoff_runtime_wake_for_agent(
                        decision.agent_id.as_str(),
                        crate::runtime::ContinuationStatusV1::Rejected,
                        "provider_module_command_not_executable",
                    )
                    .map_err(|error| AgentDecisionTrace {
                        agent_id: decision.agent_id.clone(),
                        time: self.world.state().time,
                        decision: AgentDecision::Wait,
                        llm_input: None,
                        llm_output: None,
                        llm_diagnostics: None,
                        llm_error: Some(error),
                        parse_error: None,
                        llm_effect_intents: Vec::new(),
                        llm_effect_receipts: Vec::new(),
                        llm_step_trace: Vec::new(),
                        llm_prompt_section_trace: Vec::new(),
                        llm_chat_messages: Vec::new(),
                    })?;
                }
            }
        }
        Ok(decision_trace)
    }

    fn finish_provider_transport_exhaustion(
        &mut self,
        agent_id: String,
        prior_trace: Option<AgentDecisionTrace>,
        cognition_lease: Option<crate::runtime::CognitionLeaseV1>,
    ) -> AgentDecisionTrace {
        let wake_recovery_context = self.llm_sidecar.provider_recovery_context(&agent_id);
        let reason = "failed_provider: provider transport retry budget exhausted";
        let mut trace = prior_trace.unwrap_or_else(|| AgentDecisionTrace {
            agent_id: agent_id.clone(),
            time: self.world.state().time,
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
        });
        trace.agent_id = agent_id;
        trace.decision = AgentDecision::Wait;
        trace.llm_error = Some(reason.to_string());
        trace.llm_output = Some(
            serde_json::json!({
                "provider_error": {
                    "code": "failed_provider",
                    "retryable": false,
                }
            })
            .to_string(),
        );

        if let Err(error) =
            self.release_provider_cognition_lease(trace.agent_id.as_str(), cognition_lease)
        {
            trace.llm_error = Some(format!(
                "{reason}; cognition lease release remains pending: {error}"
            ));
            return trace;
        }

        if let Err(error) = self.llm_sidecar.fail_provider_cognition_turn(
            &mut self.world,
            trace.agent_id.as_str(),
            "failed_provider",
        ) {
            if let Some(context) = self
                .llm_sidecar
                .provider_recovery_context(trace.agent_id.as_str())
            {
                self.llm_sidecar.retain_provider_recovery_pending(
                    trace.agent_id.as_str(),
                    &context,
                    format!("Runtime terminalization pending: {error}"),
                );
            }
            trace.llm_error = Some(format!(
                "{reason}; Runtime cognition terminalization failed: {error}"
            ));
            return trace;
        }

        let Some(feedback) = self.llm_sidecar.provider_failure_feedback(
            trace.agent_id.as_str(),
            "failed",
            "failed_provider",
        ) else {
            if let Some(context) = self
                .llm_sidecar
                .provider_recovery_context(trace.agent_id.as_str())
            {
                self.llm_sidecar.retain_provider_recovery_pending(
                    trace.agent_id.as_str(),
                    &context,
                    "terminal feedback context unavailable",
                );
            }
            trace.llm_error = Some(format!("{reason}; terminal feedback context unavailable"));
            return trace;
        };
        if let Err(error) =
            self.allocate_runtime_feedback(feedback, RuntimeFeedbackProjectionV1::default())
        {
            if let Some(context) = self
                .llm_sidecar
                .provider_recovery_context(trace.agent_id.as_str())
            {
                self.llm_sidecar.retain_provider_recovery_pending(
                    trace.agent_id.as_str(),
                    &context,
                    format!("terminal feedback allocation pending: {error}"),
                );
            }
            trace.llm_error = Some(format!(
                "{reason}; Runtime feedback allocation failed: {error}"
            ));
            return trace;
        }
        self.drain_provider_feedback_outbox();
        if let Err(error) = self
            .llm_sidecar
            .release_provider_turn_checked(trace.agent_id.as_str())
        {
            if let Some(context) = self
                .llm_sidecar
                .provider_recovery_context(trace.agent_id.as_str())
            {
                self.llm_sidecar.retain_provider_recovery_pending(
                    trace.agent_id.as_str(),
                    &context,
                    format!("provider release pending: {error}"),
                );
            }
            trace.llm_error = Some(format!("{reason}; provider release failed: {error}"));
            return trace;
        }
        if let Err(error) = self.handoff_runtime_wake_for_agent(
            trace.agent_id.as_str(),
            crate::runtime::ContinuationStatusV1::Rejected,
            "provider_transport_exhausted",
        ) {
            if let Some(context) = wake_recovery_context.as_ref() {
                self.llm_sidecar.retain_provider_wake_recovery_pending(
                    trace.agent_id.as_str(),
                    context,
                    crate::runtime::ContinuationStatusV1::Rejected,
                    "provider_transport_exhausted",
                );
            }
            trace.llm_error = Some(format!(
                "{}; Runtime wake handoff failed: {error}",
                trace.llm_error.take().unwrap_or_default()
            ));
        }
        trace
    }

    fn release_provider_cognition_lease(
        &mut self,
        agent_id: &str,
        lease: Option<crate::runtime::CognitionLeaseV1>,
    ) -> Result<(), String> {
        self.release_provider_cognition_lease_for_request(agent_id, lease, None)
    }

    fn settle_provider_cognition_lease_for_response_or_release(
        &mut self,
        agent_id: &str,
        lease: Option<crate::runtime::CognitionLeaseV1>,
        request: Option<&crate::simulator::ContinuousAgentRequestContextV1>,
    ) -> Result<(), String> {
        if let Some(request) = request {
            self.settle_provider_cognition_lease_for_request(agent_id, lease, Some(request))?;
            self.llm_sidecar.clear_provider_cognition_lease(agent_id);
            Ok(())
        } else {
            self.release_provider_cognition_lease_for_request(agent_id, lease, None)
        }
    }

    fn release_provider_cognition_lease_for_request(
        &mut self,
        agent_id: &str,
        lease: Option<crate::runtime::CognitionLeaseV1>,
        request: Option<&crate::simulator::ContinuousAgentRequestContextV1>,
    ) -> Result<(), String> {
        let lease = lease.or_else(|| self.llm_sidecar.provider_cognition_lease(agent_id));
        let Some(lease) = lease else {
            return Ok(());
        };
        if self.chain_link_enabled() {
            let request = request.ok_or("canonical lease release requires the original request")?;
            self.llm_sidecar
                .validate_provider_cognition_lease_for_request(
                    &self.world,
                    agent_id,
                    request,
                    &lease,
                    "release",
                )?;
            self.llm_sidecar.release_provider_lease_at_authority(
                &mut self.world,
                request,
                &lease,
            )?;
            self.llm_sidecar.clear_provider_cognition_lease(agent_id);
            return Ok(());
        }
        if let Some(runtime_lease) = self
            .world
            .cognition_economy()
            .map_err(|error| format!("cognition lease release economy read failed: {error:?}"))?
            .leases
            .get(lease.lease_id.as_str())
        {
            if runtime_lease.lease_id != lease.lease_id
                || runtime_lease.idempotency_key != lease.idempotency_key
                || runtime_lease.account_id != lease.account_id
                || runtime_lease.agent_id != lease.agent_id
                || runtime_lease.agent_session_id != lease.agent_session_id
                || runtime_lease.agent_turn_id != lease.agent_turn_id
                || runtime_lease.decision_request_id != lease.decision_request_id
                || runtime_lease.request_digest != lease.request_digest
                || runtime_lease.quote != lease.quote
                || runtime_lease.reserved_amount != lease.reserved_amount
            {
                return Err(format!(
                    "cognition lease release Runtime identity mismatch for {agent_id}"
                ));
            }
            if runtime_lease.status != crate::runtime::CognitionLeaseStatusV1::Reserved {
                // A provider Wait compensation or an earlier retry may have
                // already settled/released this exact lease. Preserve the
                // terminal Runtime result and only clear the stale sidecar
                // mirror; never turn a completed provider call into a second
                // economic release.
                self.llm_sidecar.clear_provider_cognition_lease(agent_id);
                return Ok(());
            }
        }
        if let Some(request) = request {
            self.llm_sidecar
                .validate_provider_cognition_lease_for_request(
                    &self.world,
                    agent_id,
                    request,
                    &lease,
                    "release",
                )?;
        } else {
            self.llm_sidecar
                .validate_provider_cognition_lease_for_agent(
                    &self.world,
                    agent_id,
                    &lease,
                    "release",
                )?;
        }
        self.world
            .release_cognition_lease(lease.lease_id.as_str())
            .map_err(|error| {
                format!(
                    "cognition lease release failed for {}: {error:?}",
                    lease.lease_id
                )
            })?;
        self.llm_sidecar.clear_provider_cognition_lease(agent_id);
        Ok(())
    }

    pub(super) fn settle_provider_cognition_lease_for_request(
        &mut self,
        agent_id: &str,
        lease: Option<crate::runtime::CognitionLeaseV1>,
        request: Option<&crate::simulator::ContinuousAgentRequestContextV1>,
    ) -> Result<(), String> {
        let lease = lease.or_else(|| self.llm_sidecar.provider_cognition_lease(agent_id));
        let Some(lease) = lease else {
            return Ok(());
        };
        if self.chain_link_enabled() {
            let request =
                request.ok_or("canonical lease settlement requires the original request")?;
            self.llm_sidecar
                .validate_provider_cognition_lease_for_request(
                    &self.world,
                    agent_id,
                    request,
                    &lease,
                    "settle",
                )?;
            return self.llm_sidecar.settle_provider_lease_at_authority(
                &mut self.world,
                request,
                &lease,
            );
        }
        if let Some(request) = request {
            self.llm_sidecar
                .validate_provider_cognition_lease_for_request(
                    &self.world,
                    agent_id,
                    request,
                    &lease,
                    "settle",
                )?;
        } else {
            self.llm_sidecar
                .validate_provider_cognition_lease_for_agent(
                    &self.world,
                    agent_id,
                    &lease,
                    "settle",
                )?;
        }
        let economy = self.world.cognition_economy().map_err(|error| {
            format!("cognition lease settlement economy read failed: {error:?}")
        })?;
        if let Some(runtime_lease) = economy.leases.get(lease.lease_id.as_str()) {
            if runtime_lease.lease_id != lease.lease_id
                || runtime_lease.idempotency_key != lease.idempotency_key
                || runtime_lease.account_id != lease.account_id
                || runtime_lease.agent_id != lease.agent_id
                || runtime_lease.agent_session_id != lease.agent_session_id
                || runtime_lease.agent_turn_id != lease.agent_turn_id
                || runtime_lease.decision_request_id != lease.decision_request_id
                || runtime_lease.request_digest != lease.request_digest
                || runtime_lease.quote != lease.quote
                || runtime_lease.reserved_amount != lease.reserved_amount
            {
                return Err(format!(
                    "cognition lease settlement Runtime identity mismatch for {agent_id}"
                ));
            }
            if runtime_lease.status == crate::runtime::CognitionLeaseStatusV1::Settled {
                // A Runtime receipt or a previous idempotent settlement may
                // have closed this exact lease before the provider response
                // reaches this control pass. Preserve that economic result
                // instead of converting it into a transient gameplay block.
                self.llm_sidecar.clear_provider_cognition_lease(agent_id);
                return Ok(());
            }
        }
        self.world
            .settle_cognition_lease(lease.lease_id.as_str(), lease.reserved_amount)
            .map_err(|error| {
                format!(
                    "cognition lease settlement failed for {}: {error:?}",
                    lease.lease_id
                )
            })?;
        Ok(())
    }

    fn deliver_provider_feedback_best_effort(
        &mut self,
        feedback: crate::simulator::FeedbackEnvelopeV1,
    ) {
        self.deliver_provider_feedback_with_projection(
            feedback,
            RuntimeFeedbackProjectionV1::default(),
        );
    }

    fn deliver_provider_feedback_with_projection(
        &mut self,
        feedback: crate::simulator::FeedbackEnvelopeV1,
        projection: RuntimeFeedbackProjectionV1,
    ) {
        if let Err(error) = self.allocate_runtime_feedback(feedback, projection) {
            tracing::warn!(
                error = ?error,
                "Runtime feedback allocation failed; provider feedback was not delivered"
            );
            return;
        }
        self.drain_provider_feedback_outbox();
    }

    pub(super) fn allocate_runtime_feedback(
        &mut self,
        feedback: crate::simulator::FeedbackEnvelopeV1,
        projection: RuntimeFeedbackProjectionV1,
    ) -> Result<crate::runtime::RuntimeFeedbackOutboxRecordV1, String> {
        let status = feedback.status.clone();
        let request = RuntimeFeedbackRequestV1 {
            feedback_id: feedback
                .runtime_receipt_id
                .as_ref()
                .map(|_| feedback.feedback_id.clone()),
            agent_subject: feedback.agent_subject,
            agent_session_id: feedback.agent_session_id,
            agent_turn_id: feedback.agent_turn_id,
            decision_request_id: feedback.decision_request_id,
            candidate_action_id: feedback.candidate_action_id,
            runtime_receipt_id: feedback.runtime_receipt_id,
            status: feedback.status,
            request_digest: feedback.request_digest.to_string(),
            reject_reason: canonical_feedback_reason(
                status.as_str(),
                feedback.reject_reason.as_deref(),
            ),
        };
        self.world
            .allocate_runtime_feedback_with_projection(request, projection)
            .map_err(|error| format!("Runtime feedback allocation failed: {error:?}"))
    }

    /// Drive the Runtime-owned feedback outbox without making the provider
    /// transport part of the World transaction. Claimed records remain
    /// durable and are returned to `pending` on transport failure, including
    /// across a viewer restart.
    pub(in crate::viewer::runtime_live) fn drain_provider_feedback_outbox(&mut self) {
        let pending = match self.world.pending_runtime_feedback() {
            Ok(pending) => pending,
            Err(error) => {
                tracing::warn!(
                    error = ?error,
                    "Runtime feedback outbox read failed"
                );
                return;
            }
        };
        for record in pending {
            let feedback_id = record.feedback_id.clone();
            let claimed = match self.world.claim_runtime_feedback(feedback_id.as_str()) {
                Ok(record) => record,
                Err(error) => {
                    tracing::warn!(
                        feedback_id = feedback_id.as_str(),
                        error = ?error,
                        "Runtime feedback outbox claim failed"
                    );
                    continue;
                }
            };
            let payload = match claimed.transport_payload() {
                Ok(payload) => payload,
                Err(error) => {
                    self.retry_runtime_feedback_outbox(
                        feedback_id.as_str(),
                        format!("feedback transport payload validation failed: {error}"),
                    );
                    continue;
                }
            };
            if let Err(error) =
                serde_json::from_value::<crate::simulator::FeedbackEnvelopeV1>(payload.clone())
            {
                self.retry_runtime_feedback_outbox(
                    feedback_id.as_str(),
                    format!("feedback transport payload decode failed: {error}"),
                );
                continue;
            }
            match self
                .llm_sidecar
                .deliver_provider_cognition_feedback(&payload)
            {
                Ok(()) => {
                    if let Err(error) = self.world.ack_runtime_feedback(feedback_id.as_str()) {
                        tracing::warn!(
                            feedback_id = feedback_id.as_str(),
                            error = ?error,
                            "Runtime feedback outbox acknowledgement failed"
                        );
                        self.retry_runtime_feedback_outbox(
                            feedback_id.as_str(),
                            format!("feedback acknowledgement failed: {error:?}"),
                        );
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        feedback_id = feedback_id.as_str(),
                        error = %error,
                        "provider cognition feedback delivery failed; returning to Runtime outbox"
                    );
                    self.retry_runtime_feedback_outbox(feedback_id.as_str(), error);
                }
            }
        }
    }

    fn retry_runtime_feedback_outbox(&mut self, feedback_id: &str, reason: impl Into<String>) {
        if let Err(error) = self
            .world
            .retry_runtime_feedback(feedback_id, reason.into())
        {
            tracing::warn!(
                feedback_id,
                error = ?error,
                "Runtime feedback outbox retry transition failed"
            );
        }
    }
}
