use super::*;
use crate::simulator::{
    BudgetContractV1, CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR, CONTINUOUS_AGENT_CONTEXT_VERSION,
    ContinuousAgentRequestContextV1, ContinuousAgentTurnContextV1,
    DEFAULT_PROVIDER_ACTION_SCHEMA_VERSION, DEFAULT_PROVIDER_OBSERVATION_SCHEMA_VERSION,
    DecisionRequest, Digest32, ObservationEnvelope, ProviderInteractionTarget,
    ProviderMissionContext, ProviderNavigationNode, ProviderNearbyEntity, ProviderObservation,
    ProviderRecentEvent, ProviderSelfState, RuntimeBindingV1, h_v1,
};
use oasis7_simulator::Observation;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[path = "llm_sidecar_memory_correction.rs"]
mod memory_correction;
#[path = "llm_sidecar_cognition_wait.rs"]
mod wait_admission;

#[path = "llm_sidecar_provider_observation.rs"]
mod provider_observation;
use provider_observation::{
    provider_observation_from_runtime_observation, recent_runtime_event_summaries,
};

const PROVIDER_ADAPTER_PROTOCOL_VERSION: &str = "world-simulator-provider-loopback-http-v1";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::viewer::runtime_live) struct ProviderContextState {
    pub(in crate::viewer::runtime_live) turn_context: ContinuousAgentTurnContextV1,
    pub(in crate::viewer::runtime_live) request_context: ContinuousAgentRequestContextV1,
}

#[path = "llm_sidecar_hosted_wait.rs"]
mod hosted_wait;
#[path = "llm_sidecar_wait_proposal.rs"]
mod wait_proposal;
pub(in crate::viewer::runtime_live) use hosted_wait::HostedWait;
#[path = "llm_sidecar_hosted_resume.rs"]
mod hosted_resume;
pub(in crate::viewer::runtime_live) use hosted_resume::HostedResume;

#[path = "llm_sidecar_service_admission.rs"]
mod service_admission;
pub(in crate::viewer::runtime_live) use service_admission::HostedAdmission;

#[path = "llm_sidecar_context_digests.rs"]
mod context_digests;
#[path = "llm_sidecar_resume_recovery.rs"]
mod resume_recovery;
#[path = "llm_sidecar_cognition_service.rs"]
mod service_cognition;
use context_digests::{provider_policy_context_digest, provider_wait_precondition_digest};

/// Viewer-side seam for the Runtime-owned cognition binding. The viewer does
/// not inspect or synthesize persisted authority fields; Runtime is the sole
/// source of the canonical world/manifest roots and finality lineage.
trait RuntimeBindingSource {
    fn current_runtime_binding(&self, world_id: &str) -> Result<RuntimeBindingV1, String>;
}

impl RuntimeBindingSource for RuntimeWorld {
    fn current_runtime_binding(&self, world_id: &str) -> Result<RuntimeBindingV1, String> {
        let binding = self
            .current_cognition_runtime_binding()
            .map_err(|error| format!("Runtime cognition binding unavailable: {error:?}"))?;
        if binding.world_id != world_id {
            return Err(format!(
                "Runtime cognition binding world_id mismatch: expected {world_id}, got {}",
                binding.world_id
            ));
        }
        binding
            .validate()
            .map_err(|error| format!("Runtime cognition binding invalid: {error}"))?;
        Ok(binding)
    }
}

impl RuntimeLlmSidecar {
    /// Prepare complete Runtime-bound outer requests before an async actor is
    /// started. The actor receives both the reduced behavior context and this
    /// outer V1 request atomically through `AsyncAgentRunner`.
    pub(super) fn prepare_provider_request_contexts(
        &mut self,
        world: &mut RuntimeWorld,
        kernel: &mut WorldKernel,
        world_id: &str,
    ) -> Result<(), String> {
        // The wasm runner uses this path instead of the native async poll;
        // keep durable provider notifications retryable on both lanes.
        self.flush_pending_provider_world_events();
        self.hydrate_provider_lineage(world);
        if let Some(error) = self.provider_lineage_recovery_pending.as_deref() {
            return Err(format!(
                "provider lineage recovery fenced; durable checkpoint must be repaired before dispatch: {error}"
            ));
        }
        self.settle_committed_provider_cognition_leases(world)?;
        self.release_binding_changed_provider_leases(world)?;
        #[cfg(not(target_arch = "wasm32"))]
        if let Err(error) = self.recover_pending_provider_wait(world) {
            tracing::warn!(error, "provider Wait recovery remains pending");
        }
        let provider_settings = provider_settings_from_env()?;
        let runtime_binding = if self.provider_service_required {
            self.provider_service_projection
                .as_ref()
                .and_then(|view| view.runtime_binding.clone())
                .filter(|binding| binding.world_id == world_id)
                .ok_or("authorized canonical cognition binding missing")?
        } else {
            world.current_runtime_binding(world_id)?
        };
        #[cfg(any(test, feature = "test_tier_required"))]
        if !self.provider_service_required && hosted_local_mock_test_lane_enabled(true) {
            install_hosted_local_mock_test_capability_fixtures(world, true)?;
        }
        if self
            .provider_lineage_binding
            .as_ref()
            .is_some_and(|previous| previous != &runtime_binding)
        {
            // A reconnect/reorg may advance the authoritative Runtime head
            // while an older provider context is still cached. Do not let
            // that context remain eligible for prompt-control admission or
            // for the next provider turn under the new head.
            self.provider_contexts
                .retain(|_, context| context.request_context.runtime_binding == runtime_binding);
            self.provider_retry_contexts
                .retain(|_, context| context.request_context.runtime_binding == runtime_binding);
        }
        self.provider_lineage_binding = Some(runtime_binding.clone());
        self.recover_consumed_service_resumes(&runtime_binding)?;
        let recent_event_summary = recent_runtime_event_summaries(world);
        self.release_due_provider_waits(world)?;
        if self.runner.is_none() {
            return Ok(());
        }
        let agent_ids = self
            .provider_agent_ids
            .iter()
            .filter(|agent_id| {
                !self.provider_recovery_pending.contains_key(*agent_id)
                    && !self
                        .provider_continuation_recovery_pending
                        .contains_key(*agent_id)
                    && !self.provider_transport_exhausted.contains(*agent_id)
                    && !self.provider_wake_recovery_pending.contains_key(*agent_id)
                    && !self.provider_active_turns.contains_key(*agent_id)
                    && (self.has_pending_runtime_wake_for_agent(agent_id.as_str())
                        || !self.provider_contexts.contains_key(*agent_id)
                        || self.provider_retry_contexts.contains_key(*agent_id))
            })
            .cloned()
            .collect::<Vec<_>>();

        for agent_id in agent_ids {
            if self.provider_service_required
                && !self.has_pending_service_resume_for_agent(&agent_id)
            {
                self.ensure_canonical_agent_durable_admission()?;
            }
            let settings = match provider_settings.as_ref() {
                Some(settings) => settings.clone(),
                None => continuation_support::builtin_cognition_settings(agent_id.as_str())?,
            };
            if !continuation_support::provider_actor_exists(self, agent_id.as_str()) {
                self.quarantine_missing_provider_agent(world, agent_id.as_str());
                continue;
            }
            let observation = kernel.observe(agent_id.as_str());
            let observation = match observation {
                Ok(observation) => observation,
                Err(oasis7_simulator::RejectReason::AgentNotFound { .. }) => {
                    self.quarantine_missing_provider_agent(world, agent_id.as_str());
                    continue;
                }
                Err(error) => {
                    return Err(format!("provider context observation failed: {error:?}"));
                }
            };
            let replan_cause = self.provider_stale_replan_cause(agent_id.as_str());
            let runtime_wake = self
                .pending_runtime_wakes
                .values()
                .filter(|wake| wake.agent_id == agent_id)
                .min_by_key(|wake| (wake.wake_seq, wake.wake_id.as_str()))
                .cloned();
            if let Some(wake) = runtime_wake.as_ref() {
                let continuation = self.authority_continuation_for_wake(world, wake)?;
                #[cfg(not(target_arch = "wasm32"))]
                self.ensure_runtime_harness_continuation(
                    agent_id.as_str(),
                    &continuation,
                    wake.wake_id.as_str(),
                    world,
                )?;
                if continuation.remaining_budget.value == 1 {
                    // A positive simulator proposal cannot represent a final
                    // zero-budget resume. Let Runtime consume that last unit
                    // atomically instead of rejecting the wake after it has
                    // been selected (which would leave a ghost continuation).
                    let runtime_context =
                        self.authority_continuation_context(world, wake.continuation_id.as_str())?;
                    let consumption = if self.provider_service_required {
                        let original = self
                            .provider_contexts
                            .get(agent_id.as_str())
                            .or_else(|| self.provider_active_turns.get(agent_id.as_str()))
                            .cloned()
                            .ok_or(
                                "canonical final wake budget requires original provider context",
                            )?;
                        let receipt = self.provider_scheduler_operation(&original.request_context,&format!("consume:{}",wake.wake_id),crate::world_service::wire::SchedulerOperationV1::ConsumeContinuationBudget {
                            continuation_id:continuation.continuation_id.clone(),budget_spent:1,current_context:runtime_context.clone(),
                        })?;
                        serde_json::from_value::<crate::runtime::CognitionBudgetConsumptionV1>(
                            receipt,
                        )
                        .map_err(|error| {
                            format!("canonical wake budget receipt invalid: {error}")
                        })?
                    } else {
                        world
                        .consume_cognition_continuation_budget_with_context(
                            continuation.continuation_id.as_str(),
                            1,
                            runtime_context.clone(),
                        )
                        .map_err(|error| {
                            format!(
                                "Runtime final continuation budget consumption rejected {}: {error:?}",
                                wake.wake_id
                            )
                        })?
                    };
                    #[cfg(not(target_arch = "wasm32"))]
                    if let Some(runner) = self
                        .runner
                        .as_mut()
                        .and_then(RuntimeDecisionRunner::async_runner_mut)
                    {
                        // Runtime owns the terminal status, budget and status
                        // digest. Carry that exact transition into the
                        // Harness before clearing the Viewer mirrors; a
                        // generic local invalidation would misclassify the
                        // completed continuation as expired.
                        if consumption.status != crate::runtime::ContinuationStatusV1::Completed
                            || consumption.remaining_budget.value != 0
                        {
                            return Err(format!(
                                "Runtime final continuation projection is not terminal for {}",
                                wake.wake_id
                            ));
                        }
                        let mut terminal_projection = continuation.clone();
                        terminal_projection.remaining_budget = consumption.remaining_budget.clone();
                        terminal_projection.status = consumption.status;
                        terminal_projection.logical_tick = world.state().time;
                        terminal_projection.continuation_status_digest =
                            Some(consumption.continuation_status_digest.clone());
                        terminal_projection.terminal_disposition =
                            Some("budget_exhausted".to_string());
                        terminal_projection
                            .validate_authoritative()
                            .map_err(|error| {
                                format!(
                                    "Runtime final continuation projection invalid for {}: {error}",
                                    wake.wake_id
                                )
                            })?;
                        let authority = crate::simulator::ContinuationAuthorityContextV1 {
                            baseline_observation_digest: runtime_context
                                .baseline_observation_digest,
                            goal_digest: runtime_context.goal_digest,
                            policy_digest: runtime_context.policy_digest,
                            precondition_digest: runtime_context.precondition_digest,
                        };
                        runner
                            .apply_runtime_terminal_continuation_projection(
                                agent_id.as_str(),
                                terminal_projection,
                                &authority,
                            )
                            .map_err(|error| {
                                format!(
                                    "Harness final continuation projection failed {}: {error}",
                                    wake.wake_id
                                )
                            })?;
                    }
                    self.provider_continuation_proposals
                        .remove(continuation.continuation_proposal_id.as_str());
                    self.provider_continuation_recovery_pending
                        .remove(agent_id.as_str());
                    self.pending_runtime_wakes.remove(&wake.wake_id);
                    self.provider_contexts.remove(agent_id.as_str());
                    self.provider_active_turns.remove(agent_id.as_str());
                    self.provider_retry_contexts.remove(agent_id.as_str());
                    self.provider_cognition_leases.remove(agent_id.as_str());
                    self.provider_wait_until.remove(agent_id.as_str());
                    self.persist_provider_lineage_best_effort();
                    continue;
                }
            }
            let context = if runtime_wake.is_none()
                && let Some(mut retry) = self.provider_retry_contexts.remove(&agent_id)
            {
                if retry.request_context.transport_attempt >= MAX_PROVIDER_TRANSPORT_ATTEMPTS {
                    self.provider_transport_exhausted.insert(agent_id.clone());
                    self.persist_provider_lineage_best_effort();
                    continue;
                }
                retry.request_context.transport_attempt =
                    retry.request_context.transport_attempt.saturating_add(1);
                retry
            } else {
                // An ambiguous canonical resume retains its exact request
                // identity. Advancing the local sequence would create another
                // resume instead of looking up the original signed phase.
                let pending_resume_sequence =
                    self.pending_service_resume_sequence(&agent_id, runtime_wake.as_ref());
                let sequence = self
                    .provider_context_seq
                    .entry(agent_id.clone())
                    .or_insert(1);
                let current_sequence = runtime_wake
                    .as_ref()
                    .map(|wake| {
                        let next = pending_resume_sequence.unwrap_or_else(|| {
                            (*sequence).max(wake.retry_seq.saturating_add(1)).max(1)
                        });
                        *sequence = next.saturating_add(1);
                        next
                    })
                    .unwrap_or((*sequence).max(1));
                if runtime_wake.is_none() {
                    *sequence = current_sequence.saturating_add(1);
                }
                let capability_context = if self.provider_service_required {
                    let context = self
                        .provider_service_projection
                        .as_ref()
                        .and_then(|view| view.agent_context.as_ref())
                        .filter(|context| context.agent_id == agent_id)
                        .ok_or("authorized canonical Agent capability context missing")?;
                    let catalog = context.capability_catalog.clone();
                    let invocation = context.capability_invocation_context.clone();
                    let session_id = invocation
                        .presenter
                        .session_id
                        .clone()
                        .filter(|id| !id.trim().is_empty())
                        .ok_or("canonical provider session missing")?;
                    ProviderCapabilityContext {
                        catalog,
                        invocation,
                        session_id,
                    }
                } else {
                    provider_capability_context(
                        world,
                        &runtime_binding,
                        agent_id.as_str(),
                        current_sequence,
                    )?
                };
                let goal_snapshot =
                    trusted_provider_goal_snapshot(self.prompt_profiles.get(agent_id.as_str()))?;
                let session_id = runtime_wake
                    .as_ref()
                    .map(|_| {
                        format!(
                            "{}-resume-{}",
                            capability_context.session_id, current_sequence
                        )
                    })
                    .unwrap_or_else(|| {
                        self.provider_session_ids
                            .entry(agent_id.clone())
                            .or_insert_with(|| capability_context.session_id.clone())
                            .clone()
                    });
                let (mut runtime_continuation, mut runtime_resume_proposal) = runtime_wake
                    .as_ref()
                    .map(|wake| {
                        runtime_continuation_for_wake_with_identity(
                            world,
                            self.provider_service_projection
                                .as_ref()
                                .filter(|_| self.provider_service_required),
                            wake,
                            session_id.as_str(),
                            current_sequence,
                        )
                    })
                    .transpose()?
                    .map_or((None, None), |(simulator, runtime)| {
                        (Some(simulator), Some(runtime))
                    });
                let observation_for_context = observation.clone();
                let (mut turn_context, mut request_context) =
                    build_provider_context(ProviderContextInput {
                        session_id: session_id.as_str(),
                        sequence: current_sequence,
                        agent_id: agent_id.as_str(),
                        observation: observation.clone(),
                        settings: &settings,
                        runtime_binding: runtime_binding.clone(),
                        recent_event_summary: recent_event_summary.as_slice(),
                        capability_context,
                        replan_cause: replan_cause.as_ref(),
                        continuation: runtime_continuation.as_ref(),
                        memory_store: &self.provider_memory_store,
                        goal_snapshot,
                    })?;
                let pending_resume = self.validated_pending_service_resume(
                    &agent_id,
                    runtime_wake.as_ref(),
                    &request_context,
                )?;
                if let Some(pending) = pending_resume.as_ref() {
                    let original = pending
                        .resume_context
                        .as_ref()
                        .expect("validated resume context");
                    runtime_continuation = original.turn_context.continuation.clone();
                    runtime_resume_proposal = Some(Self::pending_service_resume_proposal(pending)?);
                    turn_context = original.turn_context.clone();
                    request_context = original.request_context.clone();
                }
                self.provider_memory_store
                    .bind_corrections_to_decision(&turn_context)
                    .map_err(|error| error.to_string())?;
                if let Some(cause) = replan_cause.as_ref() {
                    self.rebind_memory_corrections_for_stale_replan(cause, &turn_context)?;
                }
                if let Some(identity) =
                    lineage_generation_recovery::provider_request_capability_identity(
                        &request_context,
                    )
                {
                    self.provider_capability_identities
                        .insert(agent_id.clone(), identity);
                }
                if let (Some(wake), Some(proposal)) =
                    (runtime_wake.as_ref(), runtime_resume_proposal)
                {
                    let predecessor_proposal_id = self
                        .authority_continuation_for_wake(world, wake)?
                        .continuation_proposal_id;
                    let next_proposal = runtime_continuation
                        .as_ref()
                        .expect("Runtime resume always produces a next Harness proposal")
                        .clone();
                    self.provider_continuation_proposals.insert(
                        next_proposal.continuation_proposal_id.clone(),
                        next_proposal.clone(),
                    );
                    if let Err(error) = self.persist_provider_lineage() {
                        self.provider_continuation_proposals
                            .remove(next_proposal.continuation_proposal_id.as_str());
                        return Err(format!(
                            "provider continuation lineage persistence failed before Runtime wake resume: {error}"
                        ));
                    }
                    let resume = crate::runtime::CognitionContinuationResumeRequestV1 {
                        agent_session_id: request_context.agent_session_id.clone(),
                        agent_turn_id: request_context.agent_turn_id.clone(),
                        decision_request_id: request_context.decision_request_id.clone(),
                        request_digest: request_context.request_digest.to_string(),
                        context_digest: async_support::runtime_provider_context_digest(
                            &request_context,
                        ),
                    };
                    let mut current_context =
                        crate::simulator::ContinuationCurrentContextV1::from_observation(
                            observation_for_context,
                            &turn_context.goal_snapshot,
                            provider_policy_context_digest(&request_context),
                            provider_wait_precondition_digest(&observation),
                        );
                    if let Some(pending) = pending_resume.as_ref() {
                        current_context = pending.resume_current_context.clone().ok_or(
                            "pending canonical ResumeWake current context missing; fenced",
                        )?;
                    }
                    let resumed = if self.provider_service_required {
                        let operation = pending_resume
                            .as_ref()
                            .map(|pending| {
                                let crate::world_service::wire::WorldServicePayloadV1::Scheduler(
                                    signed,
                                ) = &pending.payload
                                else {
                                    unreachable!()
                                };
                                signed.request.operation.clone()
                            })
                            .unwrap_or_else(|| {
                                crate::world_service::wire::SchedulerOperationV1::ResumeWake {
                                    wake_id: wake.wake_id.clone(),
                                    proposal,
                                    budget_spent: 1,
                                    resume,
                                    current_context: crate::runtime::CognitionContextDigestsV1 {
                                        baseline_observation_digest: current_context
                                            .authority
                                            .baseline_observation_digest
                                            .clone(),
                                        goal_digest: current_context.authority.goal_digest.clone(),
                                        policy_digest: current_context
                                            .authority
                                            .policy_digest
                                            .clone(),
                                        precondition_digest: current_context
                                            .authority
                                            .precondition_digest
                                            .clone(),
                                    },
                                }
                            });
                        let receipt = self.provider_scheduler_operation_with_resume_context(
                            &request_context,
                            &format!("resume:{}", wake.wake_id),
                            operation,
                            Some((
                                ProviderContextState {
                                    turn_context: turn_context.clone(),
                                    request_context: request_context.clone(),
                                },
                                current_context.clone(),
                            )),
                        )?;
                        serde_json::from_value::<crate::runtime::CognitionWakeHandoffResultV1>(
                            receipt,
                        )
                        .map_err(|error| {
                            format!("canonical wake resume receipt invalid: {error}")
                        })?
                    } else {
                        match world.resume_cognition_wake_with_context(
                            &wake.wake_id,
                            proposal,
                            1,
                            resume,
                            crate::runtime::CognitionContextDigestsV1 {
                                baseline_observation_digest: current_context
                                    .authority
                                    .baseline_observation_digest
                                    .clone(),
                                goal_digest: current_context.authority.goal_digest.clone(),
                                policy_digest: current_context.authority.policy_digest.clone(),
                                precondition_digest: current_context
                                    .authority
                                    .precondition_digest
                                    .clone(),
                            },
                        ) {
                            Ok(result) => result,
                            Err(error) => {
                                // A stale wake must not remain leased just
                                // because the current-context gate rejected it.
                                // Runtime's terminal handoff is scoped to this
                                // exact wake; local mirrors are then removed for
                                // this Agent only.
                                return Err(self.handle_provider_wake_resume_failure(
                                    world,
                                    wake,
                                    &current_context,
                                    &turn_context,
                                    &request_context,
                                    predecessor_proposal_id.as_str(),
                                    next_proposal.continuation_proposal_id.as_str(),
                                    agent_id.as_str(),
                                    &error,
                                ));
                            }
                        }
                    };
                    #[cfg(not(target_arch = "wasm32"))]
                    if let Some(runner) = self
                        .runner
                        .as_mut()
                        .and_then(RuntimeDecisionRunner::async_runner_mut)
                    {
                        runner
                            .reconcile_runtime_wake_with_current_context(
                                agent_id.as_str(),
                                &current_context,
                                &resumed.continuation,
                                resumed
                                    .replanned_continuation
                                    .as_ref()
                                    .and_then(|_| runtime_continuation.clone()),
                            )
                            .map_err(|error| {
                                format!(
                                    "Harness wake reconciliation failed {}: {error}",
                                    wake.wake_id
                                )
                            })?;
                    }
                    self.provider_continuation_proposals
                        .remove(predecessor_proposal_id.as_str());
                    // The selected wake has been consumed. Its re-planned
                    // continuation is now Runtime-active and will be selected
                    // by the normal scheduler on a later tick; never retain
                    // the consumed lease in the adapter's mirror.
                    self.pending_runtime_wakes.remove(&wake.wake_id);
                    self.provider_scheduler_pending.remove(&format!(
                        "{}:resume:{}",
                        request_context.provider_invocation_key(),
                        wake.wake_id
                    ));
                }
                ProviderContextState {
                    turn_context,
                    request_context,
                }
            };
            self.provider_contexts.insert(agent_id.clone(), context);
            if replan_cause.is_some() {
                self.mark_provider_stale_replan_dispatched(agent_id.as_str());
            }
            self.persist_provider_lineage_best_effort();
        }
        Ok(())
    }
}

impl RuntimeLlmSidecar {
    fn quarantine_missing_provider_agent(&mut self, world: &mut RuntimeWorld, agent_id: &str) {
        if self.provider_service_required {
            self.provider_continuation_recovery_pending.insert(
                agent_id.into(),
                "canonical provider actor or observation is unavailable".into(),
            );
            self.persist_provider_lineage_best_effort();
            return;
        }
        self.provider_agent_ids.remove(agent_id);
        self.provider_session_ids.remove(agent_id);
        self.provider_context_seq.remove(agent_id);
        self.provider_contexts.remove(agent_id);
        self.provider_retry_contexts.remove(agent_id);
        self.provider_active_turns.remove(agent_id);
        self.provider_cognition_leases.remove(agent_id);
        self.provider_recovery_pending.remove(agent_id);
        self.provider_continuation_recovery_pending.remove(agent_id);
        self.provider_continuation_proposals
            .retain(|_, proposal| proposal.agent_id != agent_id);
        self.provider_wait_until.remove(agent_id);
        self.provider_stale_replans.remove(agent_id);
        self.provider_transport_exhausted.remove(agent_id);
        self.provider_held_decisions.remove(agent_id);

        let wake_ids = self
            .pending_runtime_wakes
            .values()
            .filter(|wake| wake.agent_id == agent_id)
            .map(|wake| wake.wake_id.clone())
            .collect::<Vec<_>>();
        for wake_id in wake_ids {
            // Close only this missing Agent's lease.  A sibling Agent's wake
            // must remain schedulable and must not be consumed as cleanup.
            if let Err(error) = world.handoff_cognition_wake(
                wake_id.as_str(),
                crate::runtime::CognitionWakeDispositionV1::Terminal {
                    status: crate::runtime::ContinuationStatusV1::Rejected,
                    reason: "provider_actor_missing".to_string(),
                },
            ) {
                tracing::warn!(
                    agent_id,
                    wake_id,
                    error = ?error,
                    "missing provider actor wake cleanup failed"
                );
            }
            self.pending_runtime_wakes.remove(wake_id.as_str());
        }
        let event_keys = self
            .pending_provider_world_events
            .iter()
            .filter(|(_, pending)| pending.agent_id == agent_id)
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in event_keys {
            self.pending_provider_world_events.remove(key.as_str());
            self.provider_world_event_quarantine
                .insert(key, format!("provider_actor_missing:{agent_id}"));
        }
        self.persist_provider_lineage_best_effort();
    }
}

struct ProviderContextInput<'a> {
    session_id: &'a str,
    sequence: u64,
    agent_id: &'a str,
    observation: Observation,
    settings: &'a ProviderDecisionSettings,
    runtime_binding: RuntimeBindingV1,
    recent_event_summary: &'a [String],
    capability_context: ProviderCapabilityContext,
    replan_cause: Option<&'a ProviderStaleReplanCause>,
    continuation: Option<&'a SimulatorContinuationProposalV1>,
    memory_store: &'a MemoryWriteStore,
    goal_snapshot: crate::simulator::GoalSnapshotV1,
}

fn build_provider_context(
    input: ProviderContextInput<'_>,
) -> Result<
    (
        ContinuousAgentTurnContextV1,
        ContinuousAgentRequestContextV1,
    ),
    String,
> {
    let ProviderContextInput {
        session_id,
        sequence,
        agent_id,
        observation,
        settings,
        runtime_binding,
        recent_event_summary,
        capability_context,
        replan_cause,
        continuation,
        memory_store,
        goal_snapshot,
    } = input;
    let action_catalog = provider_phase1_action_catalog();
    let memory_snapshot = memory_store.context_snapshot(agent_id, session_id, "session_private", 8);
    let memory_summary =
        provider_memory_summary_with_snapshot(provider_phase1_memory_summary(), &memory_snapshot);
    let mut recent_event_summary = recent_event_summary.to_vec();
    if let Some(cause) = replan_cause {
        recent_event_summary.push(format!(
            "stale_base_replan parent_agent_turn_id={} parent_decision_request_id={} replan_count={}",
            cause.parent_agent_turn_id, cause.parent_decision_request_id, cause.count
        ));
        let keep_from = recent_event_summary.len().saturating_sub(8);
        recent_event_summary.drain(..keep_from);
    }
    let provider_observation = provider_observation_from_runtime_observation(
        &observation,
        settings.execution_mode,
        action_catalog.as_slice(),
        recent_event_summary.as_slice(),
    );
    let capability_catalog_digest = h_v1(
        "oasis7.cognition.capability-catalog.v1",
        &capability_context.catalog,
    );
    let capability_invocation_context_digest = h_v1(
        "oasis7.cognition.capability-invocation-context.v1",
        &capability_context.invocation,
    );
    let base_decision_request = DecisionRequest {
        observation: ObservationEnvelope {
            agent_id: agent_id.to_string(),
            world_time: observation.time,
            mode: settings.execution_mode,
            observation_schema_version: DEFAULT_PROVIDER_OBSERVATION_SCHEMA_VERSION.to_string(),
            action_schema_version: DEFAULT_PROVIDER_ACTION_SCHEMA_VERSION.to_string(),
            environment_class: Some("runtime_live".to_string()),
            fallback_reason: settings.fallback_reason.clone(),
            recent_event_summary: provider_observation
                .recent_events
                .iter()
                .map(|event| event.summary.clone())
                .collect(),
            observation: provider_observation,
            memory_summary: Some(memory_summary),
            action_catalog,
            module_command_catalog: Vec::new(),
            timeout_budget_ms: settings.decision_timeout_ms,
        },
        provider_config_ref: Some(format!(
            "provider://{}/runtime-live/{}",
            settings.provider_transport, agent_id
        )),
        agent_profile: Some(settings.agent_profile.clone()),
        fixture_id: None,
        replay_id: None,
        capability_catalog: Some(capability_context.catalog),
        capability_invocation_context: Some(capability_context.invocation),
        timeout_budget_ms: settings.decision_timeout_ms,
    };
    let agent_turn_id = continuation
        .map(|value| value.agent_turn_id.clone())
        .unwrap_or_else(|| format!("{session_id}-turn-{sequence}"));
    let decision_request_id = continuation
        .map(|value| value.decision_request_id.clone())
        .unwrap_or_else(|| format!("{session_id}-request-{sequence}"));
    let mut request_context = ContinuousAgentRequestContextV1 {
        base_decision_request,
        context_discriminator: CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR.to_string(),
        context_version: CONTINUOUS_AGENT_CONTEXT_VERSION,
        protocol_version: "oasis7.continuous-agent-request.v1".to_string(),
        agent_session_id: session_id.to_string(),
        agent_turn_id: agent_turn_id.clone(),
        decision_request_id: decision_request_id.clone(),
        retry_seq: sequence,
        transport_attempt: 1,
        agent_subject: agent_id.to_string(),
        runtime_binding,
        observation_digest: h_v1("oasis7.cognition.observation.v1", &Value::Null),
        capability_catalog_digest,
        capability_invocation_context_digest,
        memory_snapshot_digest: Digest32::from(memory_snapshot.digest.clone()),
        goal_snapshot_digest: Digest32::from(goal_snapshot.digest.clone()),
        continuation_digest: continuation
            .map(|value| h_v1("oasis7.cognition.continuation.v1", value))
            .unwrap_or_else(|| h_v1("oasis7.cognition.continuation.v1", &Value::Null)),
        adapter_protocol_version: PROVIDER_ADAPTER_PROTOCOL_VERSION.to_string(),
        budget_contract: BudgetContractV1 {
            max_latency_ms: settings.decision_timeout_ms,
            max_repair_attempts: 0,
            max_model_calls: 4,
            max_tool_calls: 3,
        },
        request_digest: Digest32::default(),
    };
    request_context.observation_digest = h_v1(
        "oasis7.cognition.observation.v1",
        &request_context.base_decision_request.observation,
    );
    request_context.request_digest = request_context.request_digest();
    request_context
        .validate_production_lane()
        .map_err(|error| format!("provider request context invalid: {error}"))?;
    let turn_context = ContinuousAgentTurnContextV1 {
        agent_id: agent_id.to_string(),
        agent_session_id: session_id.to_string(),
        agent_turn_id,
        decision_request_id,
        request_digest: request_context.request_digest.clone(),
        memory_snapshot,
        goal_snapshot,
        continuation: continuation.cloned(),
    };
    turn_context
        .validate_for_agent(agent_id)
        .map_err(|error| format!("provider turn context invalid: {error}"))?;
    Ok((turn_context, request_context))
}

fn trusted_provider_goal_snapshot(
    profile: Option<&AgentPromptProfile>,
) -> Result<crate::simulator::GoalSnapshotV1, String> {
    let revision = profile.map(|profile| profile.version).unwrap_or(0).max(1);
    let short_term_summary = profile
        .and_then(|profile| profile.short_term_goal_override.clone())
        .unwrap_or_else(runtime_live_phase1_short_term_goal);
    let long_term_summary = profile
        .and_then(|profile| profile.long_term_goal_override.clone())
        .unwrap_or_default();
    crate::simulator::GoalSnapshotProjector::project(
        Some(crate::simulator::GoalSnapshotInputV1 {
            revision,
            short_term_summary,
            long_term_summary,
            blocked_reason: None,
            // The Viewer only supplies the host-owned projection. Provider
            // output is never accepted as a goal source.
            provenance: "harness_projection".to_string(),
        }),
        None,
    )
    .map_err(|error| format!("trusted provider goal snapshot invalid: {error}"))
}

fn provider_memory_summary_with_snapshot(
    mut base: String,
    snapshot: &crate::simulator::MemoryContextSnapshotV1,
) -> String {
    if snapshot.entries.is_empty() {
        return base;
    }
    base.push_str("\ncommitted_memory_snapshot:");
    for entry in &snapshot.entries {
        base.push_str("\n- ");
        base.push_str(entry.summary.as_str());
        if !entry.tags.is_empty() {
            base.push_str(" [");
            base.push_str(entry.tags.join(",").as_str());
            base.push(']');
        }
    }
    // Keep the legacy provider request bounded even when all individual
    // memory intents satisfy their per-entry limits.
    const MAX_SUMMARY_BYTES: usize = 4096;
    if base.len() > MAX_SUMMARY_BYTES {
        let mut end = MAX_SUMMARY_BYTES;
        while !base.is_char_boundary(end) {
            end -= 1;
        }
        base.truncate(end);
    }
    base
}

pub(super) fn active_runtime_continuation_for_wake(
    world: &RuntimeWorld,
    wake: &crate::runtime::SchedulerWakeV1,
) -> Result<crate::runtime::AgentContinuation, String> {
    // Runtime owns continuation identity and lifecycle. Keep the provider
    // context overlay below for the simulator-only proposal fields, but never
    // select a continuation from its raw persistence projection.
    world
        .active_cognition_continuations()
        .map_err(|error| format!("Runtime continuation readback failed: {error:?}"))?
        .into_iter()
        .find(|continuation| {
            continuation.continuation_id == wake.continuation_id
                && continuation.wake_id == wake.wake_id
                && continuation.agent_id == wake.agent_id
        })
        .ok_or_else(|| {
            format!(
                "Runtime continuation missing for scheduler wake {}",
                wake.wake_id
            )
        })
}

pub(super) fn runtime_context_digests_for_continuation(
    world: &RuntimeWorld,
    continuation_id: &str,
) -> Result<crate::runtime::CognitionContextDigestsV1, String> {
    let entry = world
        .cognition()
        .get("continuation_contexts")
        .and_then(Value::as_object)
        .and_then(|contexts| contexts.get(continuation_id))
        .cloned()
        .ok_or_else(|| {
            format!(
                "Runtime continuation context missing for final budget consumption {continuation_id}"
            )
        })?;
    serde_json::from_value(entry).map_err(|error| {
        format!(
            "Runtime continuation context invalid for final budget consumption {continuation_id}: {error}"
        )
    })
}

use service_cognition::runtime_continuation_for_wake_with_identity;

use crate::runtime_host::provider_context::{
    ProviderCapabilityContext, provider_capability_context,
};

#[cfg(test)]
#[path = "llm_sidecar_cognition_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "llm_sidecar_host_goal_tests.rs"]
mod host_goal_tests;
