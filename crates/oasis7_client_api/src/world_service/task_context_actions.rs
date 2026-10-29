//! Pure task action construction; no signature or authority issuance.
use super::*;

pub fn context_deadline(base_tick: u64, grant_until: u64) -> Result<DecimalU64, String> {
    let lease_until = base_tick
        .checked_add(TASK_CONTEXT_LEASE_TICKS)
        .ok_or("task context deadline overflow")?;
    if grant_until < base_tick {
        return Err("task grant expired".into());
    }
    Ok(DecimalU64(grant_until.min(lease_until)))
}

pub fn task_cognition_intent_from_public_context(
    context: &oasis7_agent_api::game::TaskContextIdentityV1,
    binding: oasis7_agent_api::world_cognition::RuntimeCognitionBaseBindingV1,
    authority: TaskAuthorityRef<'_>,
    actor_observation: &serde_json::Value,
    action: oasis7_agent_api::world_action::Action,
) -> Result<CognitionIntentV1, String> {
    context.validate()?;
    binding.validate().map_err(|e| e.to_string())?;
    authority.validate_for(&context.fence, &binding)?;
    if authority.native_authorization_root() != context.native_authorization_root.as_deref() {
        return Err("native task action authority definition changed".into());
    }
    if authority.mode() != context.authority_mode
        || authority.agent_id() != context.fence.agent_id
        || binding.world_id != context.fence.world_id
        || binding.branch_id != context.fence.branch_id
        || binding.reorg_epoch != context.fence.reorg_epoch.0
    {
        return Err("task action public authority mismatch".into());
    }
    let request = oasis7_agent_api::world_cognition::RuntimeCognitionCommitRequestV1 {
        agent_id: context.fence.agent_id.clone(),
        agent_session_id: context.agent_session_id.clone(),
        agent_turn_id: context.agent_turn_id.clone(),
        decision_request_id: context.decision_request_id.clone(),
        retry_seq: context.retry_seq,
        transport_attempt: context.transport_attempt,
        request_digest: context.request_commitment.clone(),
        observation_digest: request_digest("game-task-public-observation/v1", actor_observation)?,
        context_digest: context.context_commitment.clone(),
        capability_snapshot_hash: authority.hashes().1.to_owned(),
        authority_context_hash: authority.hashes().2.to_owned(),
        captured_base_binding: binding,
    };
    let mut response_artifact = oasis7_agent_api::world_cognition::RuntimeCognitionResponseArtifactV1 {
        schema_version: 1,
        context_discriminator:
            oasis7_agent_api::world_cognition::RuntimeCognitionResponseArtifactV1::CONTEXT_DISCRIMINATOR.into(),
        context_version: 1,
        agent_session_id: context.agent_session_id.clone(),
        agent_turn_id: context.agent_turn_id.clone(),
        decision_request_id: context.decision_request_id.clone(),
        retry_seq: context.retry_seq,
        transport_attempt: context.transport_attempt,
        request_digest: context.request_commitment.clone(),
        response_digest: request_digest(
            "game-task-public-action/v1",
            &(&context.request_commitment, &action),
        )?,
        artifact_digest: String::new(),
    };
    response_artifact.refresh_artifact_digest();
    request.validate().map_err(|e| e.to_string())?;
    response_artifact
        .validate_for_request(&request)
        .map_err(|e| e.to_string())?;
    Ok(CognitionIntentV1 {
        request,
        action,
        response_artifact,
        delegation_generation: context
            .fence
            .executor_binding
            .as_ref()
            .ok_or("task executor unavailable")?
            .delegation_generation
            .0,
        causal_proposal: None,
        task_fence: Some(context.fence.clone()),
    })
}

pub fn cognition_intent<C, S>(
    captured: &CapturedTaskContextV1<C, S>,
    action: oasis7_agent_api::world_action::Action,
) -> Result<CognitionIntentV1, String> {
    captured.prepared.validate()?;
    let current = &captured.current_public_context;
    current.validate_for(&captured.prepared.identity)?;
    task_cognition_intent_from_public_context(
        &captured.prepared.identity,
        current.captured_base_binding.clone(),
        current.authority()?,
        &current.actor_observation,
        action,
    )
}

/// Reports a terminal outcome using a distinct public scheduler correlation identity.
#[expect(
    clippy::type_complexity,
    reason = "Generic legacy scheduler slots preserve one wire schema across native and runtime profiles"
)]
pub fn outcome_intent<C, S, P, L, CP, ST, R, D, X>(
    captured: &CapturedTaskContextV1<C, S>,
    reason: &str,
) -> Result<SchedulerProfileV1<P, L, CP, ST, R, D, X>, String> {
    captured.prepared.validate()?;
    captured
        .current_public_context
        .validate_for(&captured.prepared.identity)?;
    public_outcome_intent(
        &captured.prepared.identity,
        captured
            .current_public_context
            .captured_base_binding
            .clone(),
        reason,
    )
}
/// Identity-only terminal failure; expiry cleanup needs no private body retrieval.
#[expect(
    clippy::type_complexity,
    reason = "Generic legacy scheduler slots preserve one wire schema across native and runtime profiles"
)]
pub fn public_outcome_intent<P, L, CP, ST, R, D, X>(
    identity: &TaskContextIdentityV1,
    current_base: RuntimeCognitionBaseBindingV1,
    reason: &str,
) -> Result<SchedulerProfileV1<P, L, CP, ST, R, D, X>, String> {
    identity.validate()?;
    current_base.validate().map_err(|e| e.to_string())?;
    if !matches!(
        reason,
        "provider_failed" | "transport_exhausted" | "context_expired" | "executor_aborted"
    ) || current_base.world_id != identity.fence.world_id
        || current_base.branch_id != identity.fence.branch_id
        || current_base.reorg_epoch != identity.fence.reorg_epoch.0
        || (reason == "context_expired") != (current_base.base_tick > identity.valid_until_tick.0)
    {
        return Err("task failure reason or public binding mismatch".into());
    }
    let binding = identity
        .fence
        .executor_binding
        .as_ref()
        .ok_or("executor binding missing")?;
    Ok(SchedulerIntentV1 {
        agent_id: identity.fence.agent_id.clone(),
        request_id: format!("{}:outcome", identity.decision_request_id),
        delegation_generation: binding.delegation_generation.0,
        task_prefix_binding_revision: 0,
        captured_base_binding: current_base,
        operation: SchedulerOperationV1::TaskProviderFailure {
            context: identity.clone(),
            reason: reason.into(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_task_context_deadline_never_extends_grant_or_overflows() {
        assert_eq!(context_deadline(10, 100).unwrap(), DecimalU64(26));
        assert_eq!(context_deadline(10, 12).unwrap(), DecimalU64(12));
        assert!(context_deadline(u64::MAX, u64::MAX).is_err());
        assert!(context_deadline(10, 9).is_err());
    }
}
