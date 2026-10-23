use super::*;
use crate::world_service::{request_digest, sign_service_response, signing_bytes};
use serde_json::json;
fn request() -> ParticipantOwnerViewRequestV1 {
    let ns = json!({"service_id":"identity","environment_id":"test","instance_epoch":"1"});
    serde_json::from_value(json!({
        "owner":{"request":{"api_rev":1,"client_operation_id":"register","identity_namespace":ns,"account_id":"account","world_id":"world","genesis_digest":"genesis","owner_public_key":"ab".repeat(32),"owner_key_version":"1"},"subject_public_key":"ab".repeat(32),"signature_hex":"00".repeat(64)},
        "admission":{"request":{"api_rev":1,"business_digest":"digest","identity_namespace":ns,"account_id":"account","world_id":"world","genesis_digest":"genesis","owner_public_key":"ab".repeat(32),"branch_id":"main","reorg_epoch":"0","policy_generation":"1","valid_from_tick":"0","valid_until_tick":"100"},"subject_public_key":"cd".repeat(32),"signature_hex":"00".repeat(64)},
        "read":{"request":{"contract_version":1,"observation_nonce":"ef".repeat(32),"world":{"world_id":"world","genesis_digest":"genesis"},"scope_id":"agent:agent","min_commit":null,"fixed_commit":null,"deadline_unix_ms":null},"subject_public_key":"ab".repeat(32),"signature_hex":"00".repeat(64)}
    })).unwrap()
}
#[derive(Serialize)]
struct Legacy<'a> {
    owner: &'a SignedReadRequest<GameParticipantRegisterV1>,
    admission: &'a SignedReadRequest<GameParticipantAdmissionV1>,
    read: &'a SignedReadRequest<ReadWorldViewRequest>,
}
#[test]
fn absent_profile_preserves_legacy_json_cbor_digest_and_signature() {
    let req = request();
    let old = Legacy {
        owner: &req.owner,
        admission: &req.admission,
        read: &req.read,
    };
    assert_eq!(
        serde_json::to_vec(&old).unwrap(),
        serde_json::to_vec(&req).unwrap()
    );
    assert_eq!(
        signing_bytes(PARTICIPANT_OWNER_VIEW_PATH, &old).unwrap(),
        signing_bytes(PARTICIPANT_OWNER_VIEW_PATH, &req).unwrap()
    );
    let old_digest = request_digest(PARTICIPANT_OWNER_VIEW_PATH, &old).unwrap();
    assert_eq!(
        old_digest,
        request_digest(PARTICIPANT_OWNER_VIEW_PATH, &req).unwrap()
    );
    let old_signed = sign_service_response(
        PARTICIPANT_OWNER_VIEW_PATH,
        old_digest,
        json!({"legacy":true}),
        &"09".repeat(32),
    )
    .unwrap();
    let new_signed = sign_service_response(
        PARTICIPANT_OWNER_VIEW_PATH,
        request_digest(PARTICIPANT_OWNER_VIEW_PATH, &req).unwrap(),
        json!({"legacy":true}),
        &"09".repeat(32),
    )
    .unwrap();
    assert_eq!(old_signed, new_signed);
}
#[test]
fn explicit_profile_is_bound_and_unknown_profile_rejected() {
    let mut req = request();
    let old = request_digest(PARTICIPANT_OWNER_VIEW_PATH, &req).unwrap();
    req.response_profile = Some(ParticipantOwnerViewProfileV1::OwnerControlV1);
    assert_ne!(
        old,
        request_digest(PARTICIPANT_OWNER_VIEW_PATH, &req).unwrap()
    );
    let mut value = serde_json::to_value(req).unwrap();
    value["response_profile"] = json!("unknown");
    assert!(serde_json::from_value::<ParticipantOwnerViewRequestV1>(value).is_err());
}
