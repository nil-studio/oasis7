//! Public task context capture and action metadata. Private input stays off-ledger.
use super::*;
/// Validates the public deadline against the live canonical capability catalog.
pub fn validate_task_context_identity(
    world: &World,
    context: &oasis7_agent_api::game::TaskContextIdentityV1,
    subject: &str,
    generation: u64,
    staged: &[CanonicalIntentResultV1],
) -> Result<(), String> {
    validate_task_context_identity_inner(world, context, subject, generation, staged, None)
}

fn validate_task_context_identity_inner(
    world: &World,
    context: &oasis7_agent_api::game::TaskContextIdentityV1,
    subject: &str,
    generation: u64,
    staged: &[CanonicalIntentResultV1],
    history: Option<&super::ValidatedRetentionHistory<'_>>,
) -> Result<(), String> {
    context.validate()?;
    if let Some(history) = history {
        history.validate_cognition_fence(
            &context.fence.agent_id,
            Some(&context.fence),
            subject,
            generation,
        )?;
    } else {
        validate_cognition_fence_staged(
            world,
            &context.fence.agent_id,
            Some(&context.fence),
            subject,
            generation,
            staged,
        )?;
    }
    validate_context_expiry(world, context)
}

fn validate_context_expiry(
    world: &World,
    context: &oasis7_agent_api::game::TaskContextIdentityV1,
) -> Result<(), String> {
    let authority =
        crate::World::canonical_agent_service_context(world, &context.fence.agent_id)?;
    if world.state().time > context.valid_until_tick.0
        || context.valid_until_tick.0 > authority.capability_catalog.valid_until_tick
    {
        return Err("task context expired or exceeds canonical grant expiry".into());
    }
    Ok(())
}

fn validate_context_capture_staged(
    world: &World,
    context: &oasis7_agent_api::game::TaskContextIdentityV1,
    subject: &str,
    staged: &[CanonicalIntentResultV1],
) -> Result<(), String> {
    validate_context_capture_inner(world, context, subject, staged, false, None)
}
fn validate_context_capture_inner(
    world: &World,
    context: &oasis7_agent_api::game::TaskContextIdentityV1,
    subject: &str,
    staged: &[CanonicalIntentResultV1],
    allow_expired: bool,
    history: Option<&super::ValidatedRetentionHistory<'_>>,
) -> Result<(), String> {
    let generation = context
        .fence
        .executor_binding
        .as_ref()
        .ok_or("task executor unavailable")?
        .delegation_generation
        .0;
    if allow_expired {
        context.validate()?;
        if let Some(history) = history {
            history.validate_cognition_fence(
                &context.fence.agent_id,
                Some(&context.fence),
                subject,
                generation,
            )?;
        } else {
            validate_cognition_fence_staged(
                world,
                &context.fence.agent_id,
                Some(&context.fence),
                subject,
                generation,
                staged,
            )?;
        }
    } else {
        validate_task_context_identity_inner(world, context, subject, generation, staged, history)?;
    }
    let matched = if let Some(history) = history {
        history.contains_prefix(context, subject)?
    } else {
        let mut matched = false;
        for (key, value) in &world.capability_revocation_state().world_service_results {
            correlation::validate_result(key, value)?;
            let result: CanonicalIntentResultV1 =
                serde_json::from_value(value.clone()).map_err(|_| "invalid task prefix result")?;
            matched |= matches_task_prefix(&result, context, subject);
        }
        for result in staged {
            correlation::validate_result(
                &correlation::key_digest(&result.request.correlation.key)?,
                &serde_json::to_value(result).map_err(|e| e.to_string())?,
            )?;
            matched |= matches_task_prefix(result, context, subject);
        }
        matched
    };
    if !matched {
        return Err("task context has no canonical prefix".into());
    }
    world
        .validate_active_cognition_context(
            (
                &context.fence.agent_id,
                &context.agent_session_id,
                &context.agent_turn_id,
                &context.decision_request_id,
                &context.request_commitment,
            ),
            &context.context_commitment,
            &crate::AgentDecisionEnvelopeV1::provider_invocation_key_for_request(
                &context.request_commitment,
            ),
        )
        .map_err(|_| "task context is not active".to_string())
}
fn matches_task_prefix(
    result: &CanonicalIntentResultV1,
    context: &oasis7_agent_api::game::TaskContextIdentityV1,
    subject: &str,
) -> bool {
    if result.rejected.is_some() {
        return false;
    }
    matches!(&result.request.signed_payload, WorldServicePayloadV1::Scheduler(signed)
        if signed.subject_public_key == subject && matches!(&signed.request.operation,
            SchedulerOperationV1::TaskProviderPrefix { context: captured } if captured == context))
}
pub fn validate_task_context_capture(
    world: &World,
    context: &oasis7_agent_api::game::TaskContextIdentityV1,
    subject: &str,
) -> Result<(), String> {
    validate_context_capture_staged(world, context, subject, &[])
}

pub(super) fn validate_context_capture_for_retention(
    history: &super::ValidatedRetentionHistory<'_>,
    context: &oasis7_agent_api::game::TaskContextIdentityV1,
    subject: &str,
) -> Result<(), String> {
    validate_context_capture_inner(history.world, context, subject, &[], false, Some(history))
}
/// Pure public action envelope. No provider prompt, private goal digest or reasoning.
pub fn task_cognition_intent(
    world: &World,
    context: &oasis7_agent_api::game::TaskContextIdentityV1,
    action: crate::Action,
    subject: &str,
) -> Result<CognitionIntentV1, String> {
    validate_task_context_capture(world, context, subject)?;
    let authority =
        crate::World::canonical_agent_service_context(world, &context.fence.agent_id)?;
    let actor = world
        .state()
        .agents
        .get(&context.fence.agent_id)
        .ok_or("task actor unavailable")?;
    let actor = serde_json::to_value(actor).map_err(|e| e.to_string())?;
    task_cognition_intent_from_public_context(
        context,
        world
            .current_cognition_base_binding()
            .map_err(|e| format!("{e:?}"))?,
        &authority,
        &actor,
        action,
    )
}

pub use oasis7_client_api::world_service::task_context::task_cognition_intent_from_public_context;

pub fn validate_task_commit_context(
    world: &World,
    request: &crate::RuntimeCognitionCommitRequestV1,
    fence: &oasis7_agent_api::game::GameTaskFenceV1,
    subject: &str,
    staged: &[CanonicalIntentResultV1],
) -> Result<(), String> {
    let mut prefixes = Vec::new();
    for (key, value) in &world.capability_revocation_state().world_service_results {
        correlation::validate_result(key, value)?;
        let result: CanonicalIntentResultV1 =
            serde_json::from_value(value.clone()).map_err(|_| "invalid task prefix")?;
        prefixes.push(result);
    }
    prefixes.extend_from_slice(staged);
    for result in prefixes {
        if result.rejected.is_some() {
            continue;
        }
        let WorldServicePayloadV1::Scheduler(signed) = &result.request.signed_payload else {
            continue;
        };
        let SchedulerOperationV1::TaskProviderPrefix { context } = &signed.request.operation else {
            continue;
        };
        if signed.subject_public_key == subject
            && context.fence == *fence
            && context.agent_session_id == request.agent_session_id
            && context.agent_turn_id == request.agent_turn_id
            && context.decision_request_id == request.decision_request_id
            && context.request_commitment == request.request_digest
            && context.context_commitment == request.context_digest
            && context.retry_seq == request.retry_seq
            && context.transport_attempt == request.transport_attempt
        {
            return validate_context_capture_staged(world, context, subject, staged);
        }
    }
    Err("task action has no matching canonical context".into())
}

/// Shared task gate for the core commit path, including legacy no-fence calls.
pub fn validate_task_cognition_admission(
    world: &World,
    request: &crate::RuntimeCognitionCommitRequestV1,
    authority: Option<(&oasis7_agent_api::game::GameTaskFenceV1, &str, u64)>,
    staged: &[CanonicalIntentResultV1],
) -> Result<(), String> {
    let (fence, subject, generation) = match authority {
        Some((fence, subject, generation)) => (Some(fence), subject, generation),
        None => (None, "", 0),
    };
    validate_cognition_fence_staged(world, &request.agent_id, fence, subject, generation, staged)?;
    if let Some(fence) = fence {
        validate_task_commit_context(world, request, fence, subject, staged)?;
    }
    Ok(())
}

pub fn validate_task_failure_context(
    world: &World,
    context: &oasis7_agent_api::game::TaskContextIdentityV1,
    subject: &str,
    generation: u64,
    reason: &str,
    staged: &[CanonicalIntentResultV1],
) -> Result<(), String> {
    if !matches!(
        reason,
        "provider_failed" | "transport_exhausted" | "context_expired" | "executor_aborted"
    ) {
        return Err("task failure requires bounded public code".into());
    }
    let expired = reason == "context_expired";
    if expired && world.state().time <= context.valid_until_tick.0 {
        return Err("task context has not expired".into());
    }
    if context
        .fence
        .executor_binding
        .as_ref()
        .is_none_or(|b| b.delegation_generation.0 != generation)
    {
        return Err("task failure delegation generation mismatch".into());
    }
    validate_context_capture_inner(world, context, subject, staged, expired, None)
}
