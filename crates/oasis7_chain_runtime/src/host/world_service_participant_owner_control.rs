//! Bounded current owner/task observation. Never constructs a world projection.
use super::*;
use oasis7_client_api::world_service::participant_discovery::{
    ParticipantOwnerControlResponseV1, ParticipantOwnerControlViewV1,
};

pub(super) fn read(
    service: &Service<'_>,
    request: &ParticipantOwnerViewRequestV1,
) -> Result<Value, String> {
    if service.guarded.is_some() {
        return Err("bounded owner control requires ordinary service profile".into());
    }
    verify_read_request(
        oasis7_client_api::world_service::VIEW_JSON_PATH,
        &request.read,
    )?;
    request.read.request.validate().map_err(|e| e.to_string())?;
    // Deliberate second pin and current signer check, as in the legacy read.
    let pinned = service.pin(None)?;
    let agent = service
        .scope(
            &pinned,
            &request.read.request.scope_id,
            &request.read.subject_public_key,
        )?
        .ok_or("participant owner requires Agent scope")?;
    let owner = agent_chat::owner_view(&pinned.world, &agent)?
        .ok_or("canonical participant owner unavailable")?;
    let owner = oasis7_proto::viewer::CanonicalAgentOwnerFenceV1::from(&owner);
    let task = game_task::current_control(&pinned.world, &agent)?;
    if let Some(task) = &task {
        let fence = oasis7_agent_api::game::GameTaskFenceV1::from_control(task);
        game_task::validate_task_reader(&pinned.world, &fence, &request.read.subject_public_key)?;
    }
    let view = ParticipantOwnerControlViewV1 {
        canonical_agent_owner: owner,
        game_task_control: task,
    };
    let response = ParticipantOwnerControlResponseV1 {
        schema_revision: 1,
        world: service.identity.clone(),
        version: ProjectionVersion {
            commit: pinned.commit,
            visibility_scope: request.read.request.scope_id.clone(),
            projection_revision: request_digest("projection", &view)?,
        },
        logical_tick: pinned.world.state().time,
        view,
    };
    response.validate(request)?;
    let signed = service.signed(PARTICIPANT_OWNER_VIEW_PATH, request, response)?;
    // Leave space for the identity transport's world_response wrapper without
    // increasing either existing HTTP boundary.
    if serde_json::to_vec(&signed)
        .map_err(|e| e.to_string())?
        .len()
        > 60 * 1024
    {
        return Err("bounded participant owner observation too large".into());
    }
    Ok(signed)
}

#[cfg(test)]
#[path = "world_service_participant_owner_control_tests.rs"]
mod tests;
