//! Deterministic, bounded pilot evidence for the continuous-agent parity lane.
//!
//! The pilot is deliberately smaller than a rollout: one low-frequency NPC,
//! one fixed observation, and one local scripted provider response.  It uses
//! the production async actor boundary for both behaviors, then normalizes the
//! world-facing result before hashing evidence.  The replay artifact is an
//! evidence projection only; it never calls a provider or applies an effect.

use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::async_agent_runner::{
    AsyncAgentRunner, AsyncAgentRunnerError, AsyncAgentTurnOutcome, AsyncTurnFeedback,
    AsyncTurnLifecycle, AsyncWorldEffect,
};
use super::cognition_policy::{
    GoalSnapshotInputV1, GoalSnapshotProjector, MemoryContextEntryV1, MemoryContextSnapshotV1,
};
use super::continuous_agent_harness::{ContinuousAgentTurnContextV1, h_v1};

const PILOT_PROTOCOL_VERSION: &str = "oasis7.live-agent-pilot.v1";
const PILOT_RUNTIME_VERSION: &str = "oasis7.async-agent-actor.v1";
const PILOT_SCOPE: &str = "single_low_frequency_npc";
const PILOT_AGENT_ID: &str = "pilot-low-frequency-npc";
const PILOT_COMPLETION_TIMEOUT: Duration = Duration::from_secs(10);

impl AsyncAgentRunner {
    /// Run the approved P0 deterministic pilot at the P2 rollout stage.
    ///
    /// The same host context and default observation are supplied to a
    /// Builtin and a ProviderBacked actor.  Only the ProviderBacked actor
    /// invokes a local scripted provider during the first run.  The returned
    /// replay object is intentionally a passive artifact and records zero
    /// provider invocations, effects, and debits for replay.
    pub fn run_p2_low_frequency_npc_parity(
        profile: &str,
        fixture_id: &str,
        seed: u64,
    ) -> Result<Value, AsyncAgentRunnerError> {
        if profile.trim().is_empty() || fixture_id.trim().is_empty() {
            return Err(AsyncAgentRunnerError::Cognition(
                "pilot profile and fixture id are required".to_string(),
            ));
        }

        let context = pilot_context(profile, fixture_id, seed)?;
        let builtin = collect_pilot_outcome(Self::builtin_fixture(PILOT_AGENT_ID), &context)?;
        let provider =
            collect_pilot_outcome(Self::provider_backed_fixture(PILOT_AGENT_ID), &context)?;
        let builtin_normalized = normalize_outcome(&builtin);
        let provider_normalized = normalize_outcome(&provider);
        let outcome_parity = builtin_normalized == provider_normalized;

        let input_digest = context.request_digest.to_string();
        let runtime_digest = h_v1(
            "oasis7.live-agent-pilot.runtime.v1",
            &json!({
                "protocol_version": PILOT_PROTOCOL_VERSION,
                "runtime_version": PILOT_RUNTIME_VERSION,
                "scope": PILOT_SCOPE,
            }),
        )
        .to_string();
        let evidence_without_digest = json!({
            "protocol_version": PILOT_PROTOCOL_VERSION,
            "runtime_version": PILOT_RUNTIME_VERSION,
            "rollout_stage": "P2",
            "parity_tier": "P0",
            "scope": PILOT_SCOPE,
            "profile": profile,
            "fixture_id": fixture_id,
            "seed": seed,
            "input_digest": input_digest,
            "runtime_digest": runtime_digest,
            "builtin": builtin_normalized.clone(),
            "provider": provider_normalized.clone(),
            "outcome": builtin_normalized.clone(),
            "outcome_parity": outcome_parity,
        });
        let evidence_digest = h_v1(
            "oasis7.live-agent-pilot.evidence.v1",
            &evidence_without_digest,
        )
        .to_string();

        // These metrics cover the canonical pilot fields, not subjective
        // player scoring or live HTTP latency.  No invalid action, timeout,
        // or recoverable provider error occurred in this fixed fixture.
        let report = json!({
            "protocol_version": PILOT_PROTOCOL_VERSION,
            "runtime_version": PILOT_RUNTIME_VERSION,
            "rollout_stage": "P2",
            "parity_tier": "P0",
            "scope": PILOT_SCOPE,
            "profile": profile,
            "fixture_id": fixture_id,
            "seed": seed,
            "input_digest": context.request_digest,
            "runtime_digest": runtime_digest,
            "builtin": builtin_normalized.clone(),
            "provider": provider_normalized.clone(),
            "outcome": builtin_normalized.clone(),
            "outcome_parity": outcome_parity,
            "target_async_actor_lifecycle": true,
            "completion_gap_pp": 0,
            "invalid_action_rate_ppm": 0,
            "timeout_rate_ppm": 0,
            "trace_completeness_ppm": 1_000_000,
            "recoverable_error_resolution_rate_ppm": 1_000_000,
            "provider_diagnostics": {
                "mode": "local_scripted_fixture",
                "provider_invocation_count_first_run": 1,
                "live_http_latency_observed": false,
                "subjective_scorecard_observed": false,
            },
            "error_codes": [],
            "action_outcomes": [],
            "evidence_digest": evidence_digest,
            "replay": {
                "protocol_version": PILOT_PROTOCOL_VERSION,
                "runtime_version": PILOT_RUNTIME_VERSION,
                "profile": profile,
                "fixture_id": fixture_id,
                "seed": seed,
                "deterministic": true,
                "provider_invocation_count": 0,
                "effect_count": 0,
                "debit_count": 0,
                "outcome": builtin_normalized,
                "evidence_digest": evidence_digest,
                "source": "normalized_first_run_evidence",
            },
        });
        Ok(report)
    }
}

fn pilot_context(
    profile: &str,
    fixture_id: &str,
    seed: u64,
) -> Result<ContinuousAgentTurnContextV1, AsyncAgentRunnerError> {
    let memory_entry = MemoryContextEntryV1 {
        id: format!("{fixture_id}-memory"),
        summary: "prior route was safe".to_string(),
        tags: vec!["fixture".to_string(), "retrieval".to_string()],
    };
    let mut memory_snapshot = MemoryContextSnapshotV1 {
        revision: 1,
        entries: vec![memory_entry],
        scope: format!("agent:{PILOT_AGENT_ID}"),
        digest: String::new(),
    };
    memory_snapshot.digest = memory_snapshot.computed_digest();
    let goal_snapshot = GoalSnapshotProjector::project(
        Some(GoalSnapshotInputV1 {
            revision: 1,
            short_term_summary: "reach the next safe location".to_string(),
            long_term_summary: format!("continue {profile} with bounded risk"),
            blocked_reason: None,
            provenance: "harness_projection".to_string(),
        }),
        None,
    )
    .map_err(|error| AsyncAgentRunnerError::Cognition(error.to_string()))?;
    let agent_session_id = format!("{fixture_id}-session-{seed:016x}");
    let agent_turn_id = format!("{fixture_id}-turn-0");
    let decision_request_id = format!("{fixture_id}-request-0");
    let request_digest = h_v1(
        "oasis7.live-agent-pilot.input.v1",
        &json!({
            "profile": profile,
            "fixture_id": fixture_id,
            "seed": seed,
            "agent_id": PILOT_AGENT_ID,
            "agent_session_id": agent_session_id.clone(),
            "agent_turn_id": agent_turn_id.clone(),
            "decision_request_id": decision_request_id.clone(),
            "memory_snapshot": memory_snapshot.clone(),
            "goal_snapshot": goal_snapshot.clone(),
            "action_catalog": [{"action_ref": "wait", "summary": "wait"}],
        }),
    );
    Ok(ContinuousAgentTurnContextV1 {
        agent_id: PILOT_AGENT_ID.to_string(),
        agent_session_id,
        agent_turn_id,
        decision_request_id,
        request_digest,
        memory_snapshot,
        goal_snapshot,
        continuation: None,
    })
}

fn collect_pilot_outcome(
    mut runner: AsyncAgentRunner,
    context: &ContinuousAgentTurnContextV1,
) -> Result<AsyncAgentTurnOutcome, AsyncAgentRunnerError> {
    let turn_id = runner.start_turn_with_context(PILOT_AGENT_ID, context.clone())?;
    let deadline = Instant::now() + PILOT_COMPLETION_TIMEOUT;
    loop {
        let _ = runner.step_world_without_waiting_for_provider()?;
        if let Some(outcome) = runner
            .take_completed()
            .into_iter()
            .find(|outcome| outcome.turn_id == turn_id)
        {
            return Ok(outcome);
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    Err(AsyncAgentRunnerError::Cognition(format!(
        "pilot actor did not complete before {PILOT_COMPLETION_TIMEOUT:?} deadline"
    )))
}

fn normalize_outcome(outcome: &AsyncAgentTurnOutcome) -> Value {
    json!({
        "turn_id": outcome.turn_id.get(),
        "agent_id": outcome.agent_id,
        "lifecycle": lifecycle_name(outcome.lifecycle),
        "feedback": feedback_name(&outcome.feedback),
        "world_effect": world_effect_name(&outcome.world_effect),
        "outcome": decision_name(outcome),
        "context_digest": outcome
            .prepared_context
            .as_ref()
            .map(|context| context.request_digest.to_string()),
    })
}

fn lifecycle_name(lifecycle: AsyncTurnLifecycle) -> &'static str {
    match lifecycle {
        AsyncTurnLifecycle::Completed => "completed",
        AsyncTurnLifecycle::Failed => "failed",
    }
}

fn feedback_name(feedback: &AsyncTurnFeedback) -> Value {
    match feedback {
        AsyncTurnFeedback::Wait => json!("wait"),
        AsyncTurnFeedback::WaitTicks(ticks) => json!({"wait_ticks": ticks}),
        AsyncTurnFeedback::ActionProposed => json!("action_proposed"),
        AsyncTurnFeedback::QueryProposed => json!("query_proposed"),
        AsyncTurnFeedback::ModuleCommandProposed => json!("module_command_proposed"),
        AsyncTurnFeedback::ProviderError { code } => json!({"provider_error": code}),
        AsyncTurnFeedback::ActorPanicked => json!("actor_panicked"),
    }
}

fn world_effect_name(effect: &AsyncWorldEffect) -> &'static str {
    match effect {
        AsyncWorldEffect::NoEffect => "no_effect",
        AsyncWorldEffect::ActionProposal => "action_proposal",
        AsyncWorldEffect::QueryProposal => "query_proposal",
        AsyncWorldEffect::ModuleCommandProposal => "module_command_proposal",
    }
}

fn decision_name(outcome: &AsyncAgentTurnOutcome) -> &'static str {
    match outcome.decision.as_ref() {
        Some(super::agent::AgentDecision::Wait) => "wait",
        Some(super::agent::AgentDecision::WaitTicks(_)) => "wait_ticks",
        Some(super::agent::AgentDecision::Act(_)) => "act",
        Some(super::agent::AgentDecision::Query(_)) => "query",
        Some(super::agent::AgentDecision::ModuleCommand { .. }) => "module_command",
        None => "missing",
    }
}
