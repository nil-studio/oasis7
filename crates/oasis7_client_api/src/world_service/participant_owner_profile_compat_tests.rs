//! Prior strict response readers are independent compatibility witnesses.
use super::*;
use crate::world_service::{SignedServiceResponse, verify_service_response};
use serde::de::DeserializeOwned;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyControlView {
    canonical_agent_owner: oasis7_proto::viewer::CanonicalAgentOwnerFenceV1,
    game_task_control: Option<oasis7_agent_api::game::GameTaskControlV1>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyControlResponse {
    schema_revision: u32,
    world: WorldIdentity,
    version: crate::world_service::ProjectionVersion,
    logical_tick: u64,
    view: LegacyControlView,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacySetupResponse {
    api_version: u8,
    control: LegacyControlResponse,
    signer_delegation: Option<crate::world_service_wire::AgentSignerDelegationChangeV1>,
}

fn control() -> ParticipantOwnerControlResponseV1 {
    let view: ParticipantOwnerControlViewV1 = serde_json::from_value(json!({
        "canonical_agent_owner":{"agent_id":"agent","player_id":"owner",
            "public_key":"ab".repeat(32),"world_id":"world","reorg_epoch":0,
            "canonical_authority":{"branch_id":"main","agent_identity_generation":1},
            "current_intent_id":null},"game_task_control":null
    }))
    .unwrap();
    let mut response: ParticipantOwnerControlResponseV1 = serde_json::from_value(json!({
        "schema_revision":1,"world":{"world_id":"world","genesis_digest":"genesis"},
        "version":{"commit":{"world":{"world_id":"world","genesis_digest":"genesis"},
            "binding":{"provider_world_id":"world","branch_id":"main","finality_ref":"test",
                "reorg_generation":0,"governing_manifest_ref":"test","authority_generation":1,"permission_generation":1},
            "position":1,"execution_block_hash":"test-block","state_root_ref":"test-root"},
            "visibility_scope":"agent:agent","projection_revision":"pending"},
        "logical_tick":1,"view":view
    })).unwrap();
    refresh(&mut response);
    response
}
fn refresh(response: &mut ParticipantOwnerControlResponseV1) {
    response.version.projection_revision = request_digest("projection", &response.view).unwrap();
}
fn industry() -> ParticipantOwnerControlResponseV1 {
    let mut response = control();
    response.view.native_delegation_source_id = Some("owner-source".into());
    response.view.native_delegation_authorizations = Some(Vec::new());
    response.view.ordinary_starter =
        Some(oasis7_agent_api::world_action::OrdinaryStarterStatusV1 {
            owner_binding: Some("owner".into()),
            owner_generation: Some(1),
            ..Default::default()
        });
    refresh(&mut response);
    response
}

fn assert_old_wire<T: Serialize, L: Serialize + DeserializeOwned>(
    value: &T,
    request: &ParticipantOwnerViewRequestV1,
) {
    let json = serde_json::to_vec(value).unwrap();
    let old: L = serde_json::from_slice(&json).expect("the unchanged strict reader must accept it");
    assert_eq!(json, serde_json::to_vec(&old).unwrap());
    let cbor = serde_cbor::to_vec(value).unwrap();
    assert_eq!(cbor, serde_cbor::to_vec(&old).unwrap());
    let _: L = serde_cbor::from_slice(&cbor).unwrap();
    let packed = serde_cbor::ser::to_vec_packed(value).unwrap();
    assert_eq!(packed, serde_cbor::ser::to_vec_packed(&old).unwrap());
    let _: L = serde_cbor::from_slice(&packed).unwrap();
    assert_eq!(
        signing_bytes(PARTICIPANT_OWNER_VIEW_PATH, value).unwrap(),
        signing_bytes(PARTICIPANT_OWNER_VIEW_PATH, &old).unwrap()
    );
    assert_eq!(
        request_digest("projection", value).unwrap(),
        request_digest("projection", &old).unwrap()
    );
    let digest = request_digest(PARTICIPANT_OWNER_VIEW_PATH, request).unwrap();
    let secret = "09".repeat(32);
    let public = crate::world_service::sign_read_request("fixture", (), &secret)
        .unwrap()
        .subject_public_key;
    let signed =
        sign_service_response(PARTICIPANT_OWNER_VIEW_PATH, digest.clone(), value, &secret).unwrap();
    let old_signed: SignedServiceResponse<L> =
        serde_json::from_value(serde_json::to_value(&signed).unwrap()).unwrap();
    verify_service_response(PARTICIPANT_OWNER_VIEW_PATH, &digest, &old_signed, &public).unwrap();
    assert_eq!(
        signed.signature_hex,
        sign_service_response(PARTICIPANT_OWNER_VIEW_PATH, digest, old, &secret)
            .unwrap()
            .signature_hex
    );
}

#[test]
fn owner_v1_profiles_preserve_legacy_strict_json_cbor_digest_and_signature() {
    let mut original = request();
    original.response_profile = Some(ParticipantOwnerViewProfileV1::OwnerControlV1);
    let response = control();
    response.validate(&original).unwrap();
    assert_old_wire::<_, LegacyControlResponse>(&response, &original);
    let old_view: LegacyControlView =
        serde_json::from_value(serde_json::to_value(&response.view).unwrap()).unwrap();
    assert_eq!(
        response.version.projection_revision,
        request_digest("projection", &old_view).unwrap()
    );
    original.response_profile = Some(ParticipantOwnerViewProfileV1::OwnerSetupV1);
    let setup = OwnerSetupObservationV1 {
        api_version: 1,
        control: response,
        signer_delegation: None,
    };
    setup.validate(&original).unwrap();
    assert_old_wire::<_, LegacySetupResponse>(&setup, &original);
}

#[test]
fn owner_industry_profile_requires_every_extension_and_exact_request_binding() {
    let mut original = request();
    original.response_profile = Some(ParticipantOwnerViewProfileV1::OwnerIndustryV1);
    let response = industry();
    response.validate(&original).unwrap();
    let value = serde_json::to_value(&response).unwrap();
    assert_eq!(value["view"]["native_delegation_authorizations"], json!([]));
    assert!(serde_json::from_value::<LegacyControlResponse>(value.clone()).is_err());
    for field in [
        "native_delegation_source_id",
        "native_delegation_authorizations",
        "ordinary_starter",
    ] {
        for null in [false, true] {
            let mut missing = value.clone();
            if null {
                missing["view"][field] = serde_json::Value::Null;
            } else {
                missing["view"].as_object_mut().unwrap().remove(field);
            }
            let mut missing: ParticipantOwnerControlResponseV1 =
                serde_json::from_value(missing).unwrap();
            refresh(&mut missing);
            assert!(
                missing.validate(&original).is_err(),
                "{field} must be authenticated and complete"
            );
        }
    }
    let mut empty_source = response.clone();
    empty_source.view.native_delegation_source_id = Some(" ".into());
    refresh(&mut empty_source);
    assert!(empty_source.validate(&original).is_err());
    let mut legacy_request = original.clone();
    legacy_request.response_profile = Some(ParticipantOwnerViewProfileV1::OwnerControlV1);
    assert!(response.validate(&legacy_request).is_err());
    assert!(control().validate(&original).is_err());
    legacy_request.response_profile = Some(ParticipantOwnerViewProfileV1::OwnerSetupV1);
    assert!(
        OwnerSetupObservationV1 {
            api_version: 1,
            control: response.clone(),
            signer_delegation: None
        }
        .validate(&legacy_request)
        .is_err()
    );
    let secret = "09".repeat(32);
    let public = crate::world_service::sign_read_request("fixture", (), &secret)
        .unwrap()
        .subject_public_key;
    let digest = request_digest(PARTICIPANT_OWNER_VIEW_PATH, &original).unwrap();
    let mut signed = sign_service_response(
        PARTICIPANT_OWNER_VIEW_PATH,
        digest.clone(),
        response,
        &secret,
    )
    .unwrap();
    verify_service_response(PARTICIPANT_OWNER_VIEW_PATH, &digest, &signed, &public).unwrap();
    assert!(
        verify_service_response(
            PARTICIPANT_OWNER_VIEW_PATH,
            &request_digest(PARTICIPANT_OWNER_VIEW_PATH, &legacy_request).unwrap(),
            &signed,
            &public
        )
        .is_err()
    );
    signed.payload.view.native_delegation_authorizations = None;
    assert!(
        verify_service_response(PARTICIPANT_OWNER_VIEW_PATH, &digest, &signed, &public).is_err()
    );
}
