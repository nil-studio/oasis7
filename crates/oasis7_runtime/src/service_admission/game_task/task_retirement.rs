//! Active turn exclusivity and owner-controlled retirement, reusing Runtime lifecycle.
use super::*;
fn task_prefixes(
    world: &World,
    staged: &[CanonicalIntentResultV1],
) -> Result<Vec<oasis7_agent_api::game::TaskContextIdentityV1>, String> {
    let mut contexts = BTreeMap::new();
    let mut retain = |result: CanonicalIntentResultV1| {
        if result.rejected.is_none()
            && let WorldServicePayloadV1::Scheduler(signed) = result.request.signed_payload
            && let SchedulerOperationV1::TaskProviderPrefix { context } = signed.request.operation
        {
            contexts.insert(
                (
                    context.fence.agent_id.clone(),
                    context.agent_session_id.clone(),
                    context.agent_turn_id.clone(),
                    context.decision_request_id.clone(),
                    context.request_commitment.clone(),
                ),
                context,
            );
        }
    };
    for (key, value) in &world.capability_revocation_state().world_service_results {
        correlation::validate_result(key, value)?;
        retain(serde_json::from_value(value.clone()).map_err(|_| "invalid task prefix")?);
    }
    for result in staged {
        correlation::validate_result(
            &correlation::key_digest(&result.request.correlation.key)?,
            &serde_json::to_value(result).map_err(|e| e.to_string())?,
        )?;
        retain(result.clone());
    }
    Ok(contexts.into_values().collect())
}
fn active_context(world: &World, c: &oasis7_agent_api::game::TaskContextIdentityV1) -> bool {
    world
        .validate_active_cognition_context(
            (
                &c.fence.agent_id,
                &c.agent_session_id,
                &c.agent_turn_id,
                &c.decision_request_id,
                &c.request_commitment,
            ),
            &c.context_commitment,
            &crate::AgentDecisionEnvelopeV1::provider_invocation_key_for_request(
                &c.request_commitment,
            ),
        )
        .is_ok()
}
fn close_context(
    world: &mut World,
    c: &oasis7_agent_api::game::TaskContextIdentityV1,
) -> Result<(), String> {
    world
        .retire_cognition_context_for_task((
            &c.fence.agent_id,
            &c.agent_session_id,
            &c.agent_turn_id,
            &c.decision_request_id,
            &c.request_commitment,
        ))
        .map_err(|e| format!("{e:?}"))?;
    release_task_context_leases(world, c)
}
pub fn release_task_context_leases(
    world: &mut World,
    c: &oasis7_agent_api::game::TaskContextIdentityV1,
) -> Result<(), String> {
    release_turn_leases(
        world,
        (
            &c.fence.agent_id,
            &c.agent_session_id,
            &c.agent_turn_id,
            &c.decision_request_id,
            &c.request_commitment,
        ),
    )
}
fn release_turn_leases(
    world: &mut World,
    identity: (&str, &str, &str, &str, &str),
) -> Result<(), String> {
    let (agent, session, turn, decision, request) = identity;
    let leases = world
        .cognition_economy()
        .map_err(|e| format!("{e:?}"))?
        .leases
        .into_values()
        .filter(|lease| {
            lease.agent_id == agent
                && lease.agent_session_id == session
                && lease.agent_turn_id == turn
                && lease.decision_request_id == decision
                && lease.request_digest == request
                && lease.status == oasis7_agent_api::CognitionLeaseStatusV1::Reserved
        })
        .map(|lease| lease.lease_id)
        .collect::<Vec<_>>();
    for lease in leases {
        world
            .release_cognition_lease(&lease)
            .map_err(|e| format!("{e:?}"))?;
    }
    Ok(())
}
/// Owner control transitions close old task turns in the same candidate transaction.
pub fn retire_task_contexts(
    world: &mut World,
    agent: &str,
    staged: &[CanonicalIntentResultV1],
) -> Result<(), String> {
    // First owner binding also fences an already dispatched legacy Hosted turn.
    // Its public canonical prefix supplies identity; its private prompt is never
    // copied into a new context or retained by this retirement projection.
    let mut legacy = BTreeMap::new();
    let mut retain = |result: CanonicalIntentResultV1| {
        if result.rejected.is_none()
            && let WorldServicePayloadV1::Scheduler(signed) = result.request.signed_payload
            && let SchedulerOperationV1::ProviderPrefix {
                request,
                context_digest,
            } = signed.request.operation
            && request.agent_subject == agent
        {
            let identity = (
                request.agent_subject.clone(),
                request.agent_session_id.clone(),
                request.agent_turn_id.clone(),
                request.decision_request_id.clone(),
                request.request_digest.to_string(),
            );
            legacy.insert(
                identity,
                (
                    context_digest,
                    request.provider_invocation_key().to_string(),
                ),
            );
        }
    };
    for (key, value) in &world.capability_revocation_state().world_service_results {
        correlation::validate_result(key, value)?;
        retain(serde_json::from_value(value.clone()).map_err(|_| "invalid legacy prefix result")?);
    }
    for result in staged {
        retain(result.clone());
    }
    for ((agent, session, turn, decision, request), (context, invocation)) in legacy {
        let identity = (
            agent.as_str(),
            session.as_str(),
            turn.as_str(),
            decision.as_str(),
            request.as_str(),
        );
        if world
            .validate_active_cognition_context(identity, &context, &invocation)
            .is_ok()
        {
            world
                .retire_cognition_context_for_task(identity)
                .map_err(|e| format!("{e:?}"))?;
            release_turn_leases(world, identity)?;
        }
    }
    for context in task_prefixes(world, staged)? {
        if context.fence.agent_id == agent && active_context(world, &context) {
            close_context(world, &context)?;
        }
    }
    Ok(())
}
pub fn prepare_task_prefix(
    world: &mut World,
    context: &oasis7_agent_api::game::TaskContextIdentityV1,
    staged: &[CanonicalIntentResultV1],
) -> Result<(), String> {
    for existing in task_prefixes(world, staged)? {
        if existing.fence.agent_id != context.fence.agent_id
            || existing == *context
            || !active_context(world, &existing)
        {
            continue;
        }
        if existing.fence != context.fence || world.state().time > existing.valid_until_tick.0 {
            close_context(world, &existing)?;
        } else {
            return Err("task already has an active context".into());
        }
    }
    Ok(())
}
