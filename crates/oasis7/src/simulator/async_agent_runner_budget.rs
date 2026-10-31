use super::*;

fn is_budget_exhausted_wait(trace: &AgentDecisionTrace) -> bool {
    matches!(trace.decision, AgentDecision::Wait)
        && trace
            .llm_error
            .as_deref()
            .is_some_and(|error| error.starts_with("budget_exhausted:"))
        && trace
            .llm_step_trace
            .iter()
            .any(|step| step.step_type == "budget_admission" && step.status == "denied")
}

#[expect(
    clippy::result_large_err,
    reason = "Stable actor completion seam returns the full deterministic turn outcome."
)]
pub(super) fn normalize_completion(
    completion: ActorCompletion,
) -> Result<ActorCompletion, AsyncAgentTurnOutcome> {
    if !completion
        .decision_trace
        .as_ref()
        .is_some_and(is_budget_exhausted_wait)
    {
        return Ok(completion);
    }
    Err(AsyncAgentTurnOutcome {
        turn_id: completion.turn_id,
        agent_id: completion.agent_id,
        lifecycle: AsyncTurnLifecycle::Completed,
        feedback: AsyncTurnFeedback::Wait,
        world_effect: AsyncWorldEffect::NoEffect,
        decision: Some(AgentDecision::Wait),
        decision_trace: completion.decision_trace,
        prepared_context: completion.prepared_context,
        prepared_request_context: completion.prepared_request_context,
        prepared_response_context: completion.prepared_response_context,
        provider_original_material_refs: completion.provider_original_material_refs,
        cognition_lease: None,
        memory_write_intents: Vec::new(),
    })
}
