//! Real signed service responses must remain readable by the prior strict DTOs.
use super::*;
use oasis7_client_api::world_service::{
    ProjectionVersion, participant_discovery::OwnerSetupObservationV1, request_digest,
    verify_service_response,
};

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyView {
    canonical_agent_owner: oasis7_proto::viewer::CanonicalAgentOwnerFenceV1,
    game_task_control: Option<GameTaskControlV1>,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyControl {
    schema_revision: u32,
    world: WorldIdentity,
    version: ProjectionVersion,
    logical_tick: u64,
    view: LegacyView,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacySetup {
    api_version: u8,
    control: LegacyControl,
    signer_delegation: Option<oasis7_client_api::world_service_wire::AgentSignerDelegationChangeV1>,
}

fn verify<T: serde::Serialize + serde::de::DeserializeOwned>(
    request: &ParticipantOwnerViewRequestV1,
    bytes: &[u8],
    key: &str,
) -> SignedServiceResponse<T> {
    let signed: SignedServiceResponse<T> = serde_json::from_slice(bytes).unwrap();
    verify_service_response(
        PARTICIPANT_OWNER_VIEW_PATH,
        &request_digest(PARTICIPANT_OWNER_VIEW_PATH, request).unwrap(),
        &signed,
        key,
    )
    .unwrap();
    signed
}
fn read(endpoint: &str, request: &ParticipantOwnerViewRequestV1) -> Vec<u8> {
    reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
        .post(format!("{endpoint}{PARTICIPANT_OWNER_VIEW_PATH}"))
        .json(request)
        .send()
        .unwrap()
        .error_for_status()
        .unwrap()
        .bytes()
        .unwrap()
        .to_vec()
}

pub(super) fn exercise(
    endpoint: &str,
    control_request: &ParticipantOwnerViewRequestV1,
    control_bytes: &[u8],
    service_key: &str,
) {
    let control = verify::<LegacyControl>(control_request, control_bytes, service_key);
    assert_eq!(
        control.payload.version.projection_revision,
        request_digest("projection", &control.payload.view).unwrap()
    );
    let mut setup_request = control_request.clone();
    setup_request.response_profile = Some(ParticipantOwnerViewProfileV1::OwnerSetupV1);
    let setup_bytes = read(endpoint, &setup_request);
    let legacy_setup = verify::<LegacySetup>(&setup_request, &setup_bytes, service_key);
    assert_eq!(
        legacy_setup.payload.control.version.projection_revision,
        request_digest("projection", &legacy_setup.payload.control.view).unwrap()
    );
    let setup = verify::<OwnerSetupObservationV1>(&setup_request, &setup_bytes, service_key);
    setup.payload.validate(&setup_request).unwrap();

    let mut industry_request = control_request.clone();
    industry_request.response_profile = Some(ParticipantOwnerViewProfileV1::OwnerIndustryV1);
    let industry_bytes = read(endpoint, &industry_request);
    let industry = verify::<ParticipantOwnerControlResponseV1>(
        &industry_request,
        &industry_bytes,
        service_key,
    );
    industry.payload.validate(&industry_request).unwrap();
    assert!(
        industry
            .payload
            .view
            .native_delegation_source_id
            .as_ref()
            .is_some_and(|source| !source.is_empty())
    );
    assert!(
        industry
            .payload
            .view
            .native_delegation_authorizations
            .as_ref()
            .unwrap()
            .is_empty()
    );
    let starter = industry.payload.view.ordinary_starter.as_ref().unwrap();
    assert_eq!(
        starter.owner_binding.as_ref(),
        Some(&control.payload.view.canonical_agent_owner.player_id)
    );
    assert!(
        serde_json::from_slice::<SignedServiceResponse<LegacyControl>>(&industry_bytes).is_err()
    );
    assert!(industry.payload.validate(control_request).is_err());
    assert!(setup.payload.control.validate(&industry_request).is_err());
    assert!(
        verify_service_response(
            PARTICIPANT_OWNER_VIEW_PATH,
            &request_digest(PARTICIPANT_OWNER_VIEW_PATH, control_request).unwrap(),
            &industry,
            service_key
        )
        .is_err()
    );
}
