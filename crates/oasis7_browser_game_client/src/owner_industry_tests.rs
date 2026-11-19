//! The actual browser codec verifies the negotiated owner industry observation.
use super::*;
use serde_json::json;
use ws::participant_discovery::{
    PARTICIPANT_OWNER_VIEW_PATH, ParticipantOwnerControlResponseV1, ParticipantOwnerControlViewV1,
    ParticipantOwnerViewRequestV1,
};

fn fixture() -> (
    ParticipantOwnerViewRequestV1,
    ParticipantOwnerControlResponseV1,
) {
    let snapshot: GoalExecutionSnapshotV1 = serde_json::from_str(include_str!(
        "../../oasis7_game_client/testdata/task-context/native-snapshot.json"
    ))
    .unwrap();
    let control = &snapshot.control;
    let key = &control.owner_public_key;
    let namespace = json!({"service_id":"identity","environment_id":"test","instance_epoch":"1"});
    let request: ParticipantOwnerViewRequestV1 = serde_json::from_value(json!({
        "owner":{"request":{"api_rev":1,"client_operation_id":"register","identity_namespace":namespace,
            "account_id":"account","world_id":control.world_id,"genesis_digest":control.genesis_digest,
            "owner_public_key":key,"owner_key_version":"1"},"subject_public_key":key,"signature_hex":"00".repeat(64)},
        "admission":{"request":{"api_rev":1,"business_digest":"digest","identity_namespace":namespace,"account_id":"account",
            "world_id":control.world_id,"genesis_digest":control.genesis_digest,"owner_public_key":key,
            "branch_id":control.branch_id,"reorg_epoch":control.reorg_epoch,"policy_generation":"1",
            "valid_from_tick":"0","valid_until_tick":"100"},"subject_public_key":"cd".repeat(32),"signature_hex":"00".repeat(64)},
        "read":{"request":{"contract_version":1,"observation_nonce":"ef".repeat(32),"world":snapshot.read_commit.world,
            "scope_id":format!("agent:{}",control.agent_id),"min_commit":null,"fixed_commit":null,"deadline_unix_ms":null},
            "subject_public_key":key,"signature_hex":"00".repeat(64)},
        "response_profile":"owner_industry_v1"
    })).unwrap();
    let view: ParticipantOwnerControlViewV1 = serde_json::from_value(json!({
        "canonical_agent_owner":{"agent_id":control.agent_id,"player_id":control.owner_binding,
            "public_key":key,"world_id":control.world_id,"reorg_epoch":control.reorg_epoch.0,
            "canonical_authority":{"branch_id":control.branch_id,"agent_identity_generation":control.owner_generation.0},"current_intent_id":null},
        "game_task_control":control,"native_delegation_source_id":"fixture-owner-source",
        "native_delegation_authorizations":[],"ordinary_starter":{
            "world_binding":null,"owner_binding":control.owner_binding,"owner_generation":control.owner_generation.0,
            "eligible":false,"offers":{},"claim":null,"blocker":"fixture-has-no-public-offer"}
    })).unwrap();
    let response = ParticipantOwnerControlResponseV1 {
        schema_revision: 1,
        world: snapshot.read_commit.world.clone(),
        version: ws::ProjectionVersion {
            commit: snapshot.read_commit,
            visibility_scope: request.read.request.scope_id.clone(),
            projection_revision: ws::request_digest("projection", &view).unwrap(),
        },
        logical_tick: snapshot.captured_base_binding.base_tick,
        view,
    };
    response.validate(&request).unwrap();
    (request, response)
}

#[test]
fn browser_owner_industry_verifies_profile_and_rejects_signed_missing_extensions() {
    let (request, response) = fixture();
    let secret = "09".repeat(32);
    let public = ws::sign_read_request("fixture", (), &secret)
        .unwrap()
        .subject_public_key;
    let request_json = encode(&request).unwrap();
    let request_digest = ws::request_digest(PARTICIPANT_OWNER_VIEW_PATH, &request).unwrap();
    let signed = ws::sign_service_response(
        PARTICIPANT_OWNER_VIEW_PATH,
        request_digest.clone(),
        response.clone(),
        &secret,
    )
    .unwrap();
    let payload = verify(
        PARTICIPANT_OWNER_VIEW_PATH,
        &request_json,
        &encode(&signed).unwrap(),
        &public,
    )
    .unwrap();
    validate("owner_control", &request_json, &payload).unwrap();
    let mut legacy_request = serde_json::to_value(&request).unwrap();
    legacy_request["response_profile"] = json!("owner_control_v1");
    assert!(
        verify(
            PARTICIPANT_OWNER_VIEW_PATH,
            &legacy_request.to_string(),
            &encode(&signed).unwrap(),
            &public
        )
        .is_err()
    );
    assert!(validate("owner_control", &legacy_request.to_string(), &payload).is_err());

    for field in [
        "native_delegation_source_id",
        "native_delegation_authorizations",
        "ordinary_starter",
    ] {
        let mut incomplete = serde_json::to_value(&response).unwrap();
        incomplete["view"].as_object_mut().unwrap().remove(field);
        let mut incomplete: ParticipantOwnerControlResponseV1 =
            serde_json::from_value(incomplete).unwrap();
        incomplete.version.projection_revision =
            ws::request_digest("projection", &incomplete.view).unwrap();
        let signed = ws::sign_service_response(
            PARTICIPANT_OWNER_VIEW_PATH,
            request_digest.clone(),
            incomplete,
            &secret,
        )
        .unwrap();
        // A valid service signature does not make an incomplete profile authoritative.
        let payload = verify(
            PARTICIPANT_OWNER_VIEW_PATH,
            &request_json,
            &encode(&signed).unwrap(),
            &public,
        )
        .unwrap();
        assert!(
            validate("owner_control", &request_json, &payload).is_err(),
            "missing {field}"
        );
    }
}
