use crate::runtime::{CognitionLeaseStatusV1, CognitionLeaseV1};

use super::super::agent::{AgentDecision, AgentDecisionTrace};
use super::super::continuous_agent_harness::ContinuousAgentRequestContextV1;
use super::budget;
use super::{
    ActorCompletion, AsyncAgentRunnerError, AsyncAgentTurnOutcome, AsyncTurnFeedback,
    AsyncTurnLifecycle, AsyncWorldEffect, BehaviorTurnResult, Observation, TurnMetadata,
};
use oasis7_simulator::WorldTime;

/// Recover the structured provider error emitted by the provider-backed
/// behavior. `AgentBehavior::decide` is intentionally an infallible interface,
/// so provider failures cross the actor boundary in the decision trace.
fn provider_error_code(trace: &AgentDecisionTrace) -> Option<String> {
    let error = trace.llm_error.as_deref()?;
    let structured_code = trace
        .llm_output
        .as_deref()
        .and_then(|payload| serde_json::from_str::<serde_json::Value>(payload).ok())
        .and_then(|payload| payload.get("provider_error").cloned())
        .and_then(|provider_error| provider_error.get("code").cloned())
        .and_then(|code| code.as_str().map(str::to_owned));
    if structured_code.is_some() {
        return structured_code;
    }
    // Trace normalization is diagnostic only and retains the candidate
    // decision separately. Match the viewer's narrow overflow contract,
    // while retaining an explicit structured provider failure above.
    if error.trim() == "trace_payload_too_large" {
        return None;
    }

    error
        .split_once(':')
        .map(|(code, _)| code.trim())
        .filter(|code| !code.is_empty())
        .map(str::to_owned)
}

pub(super) fn outcome_from_completion(completion: ActorCompletion) -> AsyncAgentTurnOutcome {
    if completion.panicked {
        return AsyncAgentTurnOutcome {
            turn_id: completion.turn_id,
            agent_id: completion.agent_id,
            lifecycle: AsyncTurnLifecycle::Failed,
            feedback: AsyncTurnFeedback::ActorPanicked,
            world_effect: AsyncWorldEffect::NoEffect,
            decision: None,
            decision_trace: None,
            prepared_context: completion.prepared_context,
            prepared_request_context: completion.prepared_request_context,
            prepared_response_context: completion.prepared_response_context,
            cognition_lease: None,
            memory_write_intents: completion.memory_write_intents,
        };
    }
    let completion = match budget::normalize_completion(completion) {
        Ok(completion) => completion,
        Err(outcome) => return outcome,
    };
    if let Some(code) = completion
        .decision_trace
        .as_ref()
        .and_then(provider_error_code)
    {
        return AsyncAgentTurnOutcome {
            turn_id: completion.turn_id,
            agent_id: completion.agent_id,
            lifecycle: AsyncTurnLifecycle::Failed,
            feedback: AsyncTurnFeedback::ProviderError { code },
            world_effect: AsyncWorldEffect::NoEffect,
            decision: None,
            decision_trace: completion.decision_trace,
            prepared_context: completion.prepared_context,
            prepared_request_context: completion.prepared_request_context,
            prepared_response_context: completion.prepared_response_context,
            cognition_lease: None,
            memory_write_intents: completion.memory_write_intents,
        };
    }
    let decision = completion.decision;
    let (feedback, world_effect) = match decision.as_ref() {
        Some(AgentDecision::Wait) => (AsyncTurnFeedback::Wait, AsyncWorldEffect::NoEffect),
        Some(AgentDecision::WaitTicks(ticks)) => (
            AsyncTurnFeedback::WaitTicks(*ticks),
            AsyncWorldEffect::NoEffect,
        ),
        Some(AgentDecision::Act(_)) => (
            AsyncTurnFeedback::ActionProposed,
            AsyncWorldEffect::ActionProposal,
        ),
        Some(AgentDecision::Query(_)) => (
            AsyncTurnFeedback::QueryProposed,
            AsyncWorldEffect::QueryProposal,
        ),
        Some(AgentDecision::ModuleCommand { .. }) => (
            AsyncTurnFeedback::ModuleCommandProposed,
            AsyncWorldEffect::ModuleCommandProposal,
        ),
        None => (
            AsyncTurnFeedback::ProviderError {
                code: "decision_missing".to_string(),
            },
            AsyncWorldEffect::NoEffect,
        ),
    };
    AsyncAgentTurnOutcome {
        turn_id: completion.turn_id,
        agent_id: completion.agent_id,
        lifecycle: AsyncTurnLifecycle::Completed,
        feedback,
        world_effect,
        decision,
        decision_trace: completion.decision_trace,
        prepared_context: completion.prepared_context,
        prepared_request_context: completion.prepared_request_context,
        prepared_response_context: completion.prepared_response_context,
        cognition_lease: None,
        memory_write_intents: completion.memory_write_intents,
    }
}

pub(super) fn compatibility_outcome_from_harness(
    turn_id: super::AsyncTurnId,
    completion: oasis7_agent_harness::CompatibilityTurnOutcome<BehaviorTurnResult>,
    metadata: TurnMetadata,
) -> AsyncAgentTurnOutcome {
    let (result, provider_error) = match completion.result {
        Ok(result) => (Some(result), None),
        Err(error) => (None, Some(error)),
    };
    let result = result.unwrap_or(BehaviorTurnResult {
        decision: super::super::agent::AgentDecision::Wait,
        decision_trace: None,
        prepared_context: metadata.context.clone(),
        prepared_response_context: None,
        memory_write_intents: Vec::new(),
    });
    let mut outcome = outcome_from_completion(ActorCompletion {
        turn_id,
        agent_id: completion.agent_id,
        decision: Some(result.decision),
        decision_trace: result.decision_trace,
        prepared_context: result.prepared_context.or(metadata.context),
        prepared_request_context: None,
        prepared_response_context: result.prepared_response_context,
        memory_write_intents: result.memory_write_intents,
        panicked: provider_error
            .as_ref()
            .is_some_and(|error| error.code == "actor_panicked"),
    });
    if let Some(error) = provider_error {
        outcome.lifecycle = AsyncTurnLifecycle::Failed;
        outcome.feedback = if error.code == "actor_panicked" {
            AsyncTurnFeedback::ActorPanicked
        } else {
            AsyncTurnFeedback::ProviderError { code: error.code }
        };
        outcome.world_effect = AsyncWorldEffect::NoEffect;
        outcome.decision = None;
    }
    outcome.cognition_lease = None;
    outcome
}

pub(super) fn default_observation(agent_id: &str, time: WorldTime) -> Observation {
    Observation {
        time,
        agent_id: agent_id.to_string(),
        pos: crate::geometry::GeoPos::new(0, 0, 0),
        self_resources: Default::default(),
        visibility_range_cm: 0,
        visible_agents: Vec::new(),
        visible_locations: Vec::new(),
        module_lifecycle: Default::default(),
        module_market: Default::default(),
        power_market: Default::default(),
        social_state: Default::default(),
    }
}

pub(super) fn validate_cognition_lease_for_request(
    agent_id: &str,
    request_context: &ContinuousAgentRequestContextV1,
    lease: &CognitionLeaseV1,
    logical_tick: WorldTime,
) -> Result<(), AsyncAgentRunnerError> {
    request_context.validate().map_err(|error| {
        AsyncAgentRunnerError::Cognition(format!("cognition_request_invalid: {error}"))
    })?;
    lease.validate().map_err(|error| {
        AsyncAgentRunnerError::Cognition(format!("cognition_lease_invalid: {error}"))
    })?;
    if lease.status != CognitionLeaseStatusV1::Reserved {
        return Err(AsyncAgentRunnerError::Cognition(
            "cognition_lease_not_reserved".to_string(),
        ));
    }
    if lease.agent_id != agent_id
        || request_context.agent_subject != agent_id
        || lease.idempotency_key != request_context.provider_invocation_key().to_string()
        || lease.agent_session_id != request_context.agent_session_id
        || lease.agent_turn_id != request_context.agent_turn_id
        || lease.decision_request_id != request_context.decision_request_id
        || lease.request_digest != request_context.request_digest.to_string()
        || lease.account_id != lease.quote.payer_id
        || lease.quote.resource != "cognition_units"
        || lease.reserved_amount != 1
        || lease.quote.resource_version != crate::runtime::COGNITION_RESOURCE_VERSION_V1
        || lease.quote.purpose != "provider_cognition"
        || lease.quote.scope != "agent_turn"
        || lease.quote.policy_revision
            != crate::runtime::COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION
        || lease.quote.authority_context
            != request_context
                .capability_invocation_context_digest
                .to_string()
        || lease.quote.world_binding != request_context.runtime_binding.base_world_hash.to_string()
    {
        return Err(AsyncAgentRunnerError::Cognition(
            "cognition_lease_identity_mismatch".to_string(),
        ));
    }
    if lease.reserved_at_tick > logical_tick {
        return Err(AsyncAgentRunnerError::Cognition(
            "cognition_lease_reserved_in_future".to_string(),
        ));
    }
    if lease
        .quote
        .valid_until_tick
        .is_some_and(|expires| logical_tick > expires)
    {
        return Err(AsyncAgentRunnerError::Cognition(
            "cognition_lease_expired".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod trace_error_tests {
    use super::*;

    fn trace(error: &str) -> AgentDecisionTrace {
        AgentDecisionTrace {
            agent_id: "agent-a".into(),
            time: 0,
            decision: AgentDecision::Wait,
            llm_input: None,
            llm_output: None,
            llm_error: Some(error.into()),
            parse_error: None,
            llm_diagnostics: None,
            llm_effect_intents: vec![],
            llm_effect_receipts: vec![],
            llm_step_trace: vec![],
            llm_prompt_section_trace: vec![],
            llm_chat_messages: vec![],
        }
    }

    #[test]
    fn diagnostic_overflow_is_not_a_provider_failure() {
        assert_eq!(provider_error_code(&trace("trace_payload_too_large")), None);
        assert_eq!(
            provider_error_code(&trace("trace_payload_too_large: provider_timeout")),
            Some("trace_payload_too_large".into())
        );
        assert_eq!(
            provider_error_code(&trace("provider_timeout: unavailable")),
            Some("provider_timeout".into())
        );
        let mut failure = trace("trace_payload_too_large");
        failure.llm_output =
            Some(serde_json::json!({"provider_error": {"code": "provider_timeout"}}).to_string());
        assert_eq!(
            provider_error_code(&failure),
            Some("provider_timeout".into())
        );
    }
}
