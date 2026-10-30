//! Test-local equivalent of the existing two-read Owner gateway; no approval bypass in router.
use super::*;
use oasis7_client_api::world_service::resource_basis as wire;
pub(super) fn read(
    service: &mut IdentityService,
    sid: Option<&str>,
    request: &EntryOwnerControlRequestV1,
    now: u64,
) -> Result<Vec<u8>, StoreError> {
    let EntryOwnerOperationV1::ReadResourceGoal(q) = &request.operation else {
        return Err(reject("invalid_request"));
    };
    wire::verify_resource_goal_request(q).map_err(|_| reject("invalid_request"))?;
    if request.access.response_profile != Some(ParticipantOwnerViewProfileV1::OwnerSetupV1) {
        return Err(reject("invalid_request"));
    }
    let (account, _) = service.session(sid, now)?;
    let entry = service
        .participant_entry
        .as_ref()
        .filter(|e| e.owner_control_enabled)
        .ok_or_else(|| reject("owner_control_unavailable"))?;
    service.check_participant_subject(&request.access.owner.request, &account.account_id, entry)?;
    service.store.transaction(|tx| {
        consume_quota(tx, "entry-owner", &account.account_id, now, 60_000, 120)?;
        audit(tx, &account, "entry_owner_request", now)
    })?;
    let reply = entry.forward_owner_view(&request.access)?;
    let (owner, task, commit) = owner_observation(&request.access, &reply, true)?;
    let task = task.ok_or_else(|| reject("owner_control_context_mismatch"))?;
    if GameTaskFenceV1::from_control(&task) != q.request.expected_fence
        || q.subject_public_key != owner.public_key
        || oasis7_client_api::world_service::WorldIdentity::from(q.request.world.clone())
            != entry.trust.world
        || owner.world_id != entry.trust.world.world_id
        || owner.canonical_authority.branch_id != commit.binding.branch_id
        || owner.reorg_epoch != commit.binding.reorg_generation
    {
        return Err(reject("owner_control_context_mismatch"));
    }
    let token = session_key(
        &service.config.session_key,
        sid.ok_or_else(|| reject("session_expired"))?,
    );
    service.store.transaction(|tx| {
        valid_session(
            tx,
            &token,
            service.config.rp.identity_instance_epoch,
            super::super::now_ms(),
        )
        .map(|_| ())
    })?;
    let response = entry.forward_resource_component(wire::RESOURCE_GOAL_PATH_V2, q, |r| {
        wire::verify_resource_goal_response(q, r, &entry.trust.service_public_key)
    })?;
    service.session(sid, super::super::now_ms())?;
    wire::encode_native_resource_envelope(response).map_err(|_| reject("owner_control_unavailable"))
}
