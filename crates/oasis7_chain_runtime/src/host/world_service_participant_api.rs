//! Nonsecret policy discovery; account bindings are never included.
use super::*;
#[path = "world_service_participant_owner_control.rs"]
mod owner_control;
pub(super) use oasis7_client_api::world_service::participant_discovery::PARTICIPANT_OWNER_VIEW_PATH;
pub(super) use oasis7_client_api::world_service::participant_discovery::PARTICIPANT_POLICY_PATH;
pub(super) use oasis7_client_api::world_service::participant_discovery::PARTICIPANT_STARTER_ELIGIBILITY_PATH;
use oasis7_client_api::world_service::participant_discovery::ParticipantOwnerViewRequestV1;
use oasis7_client_api::world_service::participant_discovery::ParticipantPolicyRequestV1;

pub(super) fn starter_eligibility(
    service: &Service<'_>,
    request: ParticipantOwnerViewRequestV1,
) -> Result<Value, String> {
    let pinned = service.pin(None)?;
    let binding = &pinned.commit.binding;
    oasis7_runtime::service_admission::participant::validate_admission(
        &pinned.world,
        &request.owner,
        &request.admission,
        &binding.branch_id,
        binding.reorg_generation,
    )?;
    request.read.request.validate().map_err(|e| e.to_string())?;
    verify_read_request(
        oasis7_client_api::world_service::VIEW_JSON_PATH,
        &request.read,
    )?;
    let key = oasis7_runtime::service_admission::participant::account_key(&request.owner.request)?;
    let participant = pinned
        .world
        .capability_revocation_state()
        .participants
        .bindings
        .get(&key)
        .ok_or("participant binding unavailable")?;
    if request.read.subject_public_key != participant.owner_public_key
        || request.read.request.world != service.identity
        || request.read.request.scope_id != format!("agent:{}", participant.agent_id)
        || request.read.request.fixed_commit.is_some()
        || request.read.request.observation_nonce.is_none()
    {
        return Err("starter eligibility owner scope mismatch".into());
    }
    let eligibility = oasis7_runtime::service_admission::starter::eligibility(
        &pinned.world,
        &participant.agent_id,
        &participant.owner_public_key,
    )?;
    service.signed(
        PARTICIPANT_STARTER_ELIGIBILITY_PATH,
        &request,
        json!({"schema_revision":1,"world":service.identity,"agent_id":participant.agent_id,
        "commit":pinned.commit,"eligibility":eligibility}),
    )
}

pub(super) fn owner_view(
    service: &Service<'_>,
    request: ParticipantOwnerViewRequestV1,
) -> Result<Value, String> {
    let pinned = service.pin(None)?;
    let binding = &pinned.commit.binding;
    oasis7_runtime::service_admission::participant::validate_admission(
        &pinned.world,
        &request.owner,
        &request.admission,
        &binding.branch_id,
        binding.reorg_generation,
    )?;
    let key = oasis7_runtime::service_admission::participant::account_key(&request.owner.request)?;
    let participant = pinned
        .world
        .capability_revocation_state()
        .participants
        .bindings
        .get(&key)
        .ok_or("participant binding unavailable")?;
    if request.read.subject_public_key != participant.owner_public_key
        || request.read.request.world != service.identity
        || request.read.request.scope_id != format!("agent:{}", participant.agent_id)
        || request.read.request.fixed_commit.is_some()
        || request.read.request.observation_nonce.is_none()
    {
        return Err("participant owner read scope mismatch".into());
    }
    if request.response_profile.is_some() {
        return owner_control::read(service, &request);
    }
    // This second pin is intentional: the normal read path rechecks current
    // owner authority at delivery rather than trusting the first snapshot.
    operations::view_json(service, request.read)
}

pub(super) fn policy(
    service: &Service<'_>,
    request: SignedReadRequest<ParticipantPolicyRequestV1>,
) -> Result<Value, String> {
    verify_read_request(PARTICIPANT_POLICY_PATH, &request)?;
    request.request.validate().map_err(|e| e.to_string())?;
    if request.request.expected_world != service.identity
        || request.request.trust_config_ref != service.signer.public_key_hex
    {
        return Err("participant policy trust mismatch".into());
    }
    let pinned = service.pin(None)?;
    let mut value = pinned.world.participant_policy_view();
    value["branch_id"] = json!(pinned.commit.binding.branch_id);
    value["reorg_epoch"] = json!(pinned.commit.binding.reorg_generation.to_string());
    service.signed(PARTICIPANT_POLICY_PATH, &request, value)
}
