//! Original private task snapshot construction shared by prepare and snapshot routes.
use super::*;

pub(super) fn snapshot(
    service: &Service<'_>,
    task: &ValidatedPinnedTask<'_>,
    fence: &GameTaskFenceV1,
    authority_mode: Option<oasis7_agent_api::game::TaskContextAuthorityModeV1>,
    store: &PrivateGoalStore,
) -> Result<GoalExecutionSnapshotV1, PrivateError> {
    let pinned = task.pinned;
    let control = task
        .history
        .current_control(&fence.agent_id)?
        .ok_or(PrivateError::Forbidden)?;
    let publication = task
        .history
        .active_publish(&fence.agent_id)?
        .ok_or(PrivateError::Unavailable)?;
    let WorldServicePayloadV1::GameGoalPublish(public) = &publication.request.signed_payload else {
        return Err(PrivateError::Recovery);
    };
    let historical = service
        .pin_at_height(publication.committed_height)
        .map_err(|_| PrivateError::Recovery)?;
    let key = correlation::key_digest(&publication.request.correlation.key)?;
    if historical
        .world
        .capability_revocation_state()
        .world_service_results
        .get(&key)
        != Some(&serde_json::to_value(&publication).map_err(|_| PrivateError::Recovery)?)
    {
        return Err(PrivateError::Recovery);
    }
    let active_goal = store
        .get_prepared(&public.request)
        .map_err(|_| PrivateError::Recovery)?;
    let provider_authority = if authority_mode.is_none() {
        Some(
            pinned
                .world
                .canonical_agent_service_context(&fence.agent_id)?,
        )
    } else {
        None
    };
    let native_authority = if authority_mode.is_some() {
        Some(pinned.world.canonical_native_task_authority(fence)?)
    } else {
        None
    };
    let actor = pinned
        .world
        .state()
        .agents
        .get(&fence.agent_id)
        .ok_or(PrivateError::Forbidden)?;
    let runtime_binding = pinned
        .world
        .current_cognition_runtime_binding()
        .map_err(|_| PrivateError::Unavailable)?;
    let output = GoalExecutionSnapshotV1 {
        schema_revision: oasis7_agent_api::game::TaskContextAuthorityModeV1::schema_revision(
            authority_mode,
        ),
        control,
        read_commit: pinned.commit.clone(),
        activation_commit: historical.commit,
        active_goal,
        actor_observation_schema: "world-agent-state/v1".into(),
        actor_observation: serde_json::to_value(actor).map_err(|_| PrivateError::Unavailable)?,
        provider_authority,
        native_authority,
        runtime_binding,
        captured_base_binding: pinned
            .world
            .current_cognition_base_binding()
            .map_err(|_| PrivateError::Unavailable)?,
    };
    output.validate().map_err(|_| PrivateError::Recovery)?;
    Ok(output)
}
