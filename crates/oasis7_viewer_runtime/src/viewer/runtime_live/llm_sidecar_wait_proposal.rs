use super::*;

pub(in crate::viewer::runtime_live) fn build_hosted_wait_proposal(
    cognition: &RuntimeProviderActionContext,
    observation: Observation,
) -> Result<
    (
        SimulatorContinuationProposalV1,
        crate::runtime::CognitionContinuationProposalV1,
        crate::simulator::ContinuationCurrentContextV1,
    ),
    String,
> {
    let request = &cognition.request.request_context;
    let precondition_digest = provider_wait_precondition_digest(&observation);
    let current = crate::simulator::ContinuationCurrentContextV1::from_observation(
        observation,
        &cognition.request.turn_context.goal_snapshot,
        provider_policy_context_digest(request),
        precondition_digest,
    );
    current
        .validate_for_agent(request.agent_subject.as_str())
        .map_err(|error| format!("provider Wait current context invalid: {error}"))?;

    if let Some(bound) = &cognition.request.turn_context.continuation {
        bound
            .validate()
            .map_err(|error| format!("provider resumed Wait proposal invalid: {error}"))?;
        if bound.agent_id != request.agent_subject
            || bound.agent_session_id != request.agent_session_id
            || bound.agent_turn_id != request.agent_turn_id
            || bound.decision_request_id != request.decision_request_id
            || bound.world_id != request.runtime_binding.world_id
            || crate::simulator::h_v1(
                "oasis7.cognition.continuation.v1",
                &cognition.request.turn_context.continuation,
            ) != request.continuation_digest
        {
            return Err("provider resumed Wait bound request identity mismatch".into());
        }
        current
            .authority
            .validate_proposal(bound)
            .map_err(|error| format!("provider resumed Wait context changed: {error}"))?;
        let mut runtime: crate::runtime::CognitionContinuationProposalV1 =
            serde_json::from_value(serde_json::to_value(bound).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?;
        runtime.branch_id = request.runtime_binding.branch_id.clone();
        runtime.finality_epoch = request.runtime_binding.finality_epoch;
        runtime.finality_block_hash = request
            .runtime_binding
            .finality_block_hash
            .as_ref()
            .map(ToString::to_string);
        runtime.finality_status = request.runtime_binding.finality_status.clone();
        runtime.reorg_epoch = request.runtime_binding.reorg_epoch;
        runtime.runtime_manifest_hash = request.runtime_binding.runtime_manifest_hash.to_string();
        runtime.proposal_digest = runtime.proposal_digest();
        return Ok((bound.clone(), runtime, current));
    }

    let wait_ticks = match &cognition.response.base_decision_response.decision {
        crate::simulator::ProviderDecision::WaitTicks { ticks } => (*ticks).max(1),
        _ => 1,
    };
    let wake_tick = request.runtime_binding.base_tick.saturating_add(wait_ticks);
    let mut simulator = crate::simulator::ContinuationProposalV1 {
        schema_version: 1,
        continuation_proposal_id: format!(
            "provider-wait:{}:{}:{}",
            request.agent_subject, request.agent_turn_id, request.decision_request_id
        ),
        world_id: request.runtime_binding.world_id.clone(),
        agent_id: request.agent_subject.clone(),
        agent_session_id: request.agent_session_id.clone(),
        agent_turn_id: request.agent_turn_id.clone(),
        decision_request_id: request.decision_request_id.clone(),
        origin_turn_id: request.agent_turn_id.clone(),
        origin_request_digest: request.request_digest.to_string(),
        action_or_plan_kind: "wait".to_string(),
        action_or_envelope_digest: None,
        remaining_budget: crate::simulator::ContinuationBudgetV1 {
            unit: "ticks".to_string(),
            value: 2,
        },
        baseline_observation_digest: current.authority.baseline_observation_digest.clone(),
        goal_digest: current.authority.goal_digest.clone(),
        policy_digest: current.authority.policy_digest.clone(),
        policy_revision: cognition.request.turn_context.goal_snapshot.revision.max(1),
        precondition_summary: "provider wait until the next Runtime tick".to_string(),
        precondition_digest: current.authority.precondition_digest.clone(),
        wake_conditions: vec![crate::simulator::WakeConditionV1 {
            schema_version: "wake-condition.v1".to_string(),
            kind: "at_or_after_tick".to_string(),
            logical_tick: Some(wake_tick),
            event_digest: None,
            receipt_id: None,
            subject: None,
            path_or_rule: None,
            operator: None,
            expected_value_bytes: None,
        }],
        valid_until_tick: Some(wake_tick.saturating_add(16)),
        source: "provider_wait".to_string(),
        proposal_digest: String::new(),
    };
    simulator.proposal_digest = simulator
        .proposal_digest()
        .map_err(|error| format!("provider Wait Harness proposal invalid: {error}"))?
        .to_string();
    let runtime: crate::runtime::CognitionContinuationProposalV1 = serde_json::from_value(
        serde_json::to_value(&simulator)
            .map_err(|error| format!("provider Wait Runtime proposal encoding failed: {error}"))?,
    )
    .map_err(|error| format!("provider Wait Runtime proposal decoding failed: {error}"))?;
    // The paired schema uses the same canonical proposal digest. Runtime
    // fills branch/finality/manifest fields, which are intentionally excluded
    // from the digest domain.
    let mut runtime = runtime;
    runtime.proposal_digest = runtime.proposal_digest();

    Ok((simulator, runtime, current))
}
