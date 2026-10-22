//! A fresh committed participant has owner read authority before any provider session.
use super::*;
use oasis7_client_api::world_service::participant_discovery::{
    PARTICIPANT_OWNER_VIEW_PATH, ParticipantOwnerViewRequestV1,
};
use oasis7_client_api::world_service::{
    ReadWorldViewRequest, ReadWorldViewResponse, SignedServiceResponse, VIEW_JSON_PATH,
};
use serde_json::Value;
pub(super) fn exercise(
    endpoint: &str,
    root: &Path,
    identity: &WorldIdentity,
    signer: &crate::feedback_submit_api::FeedbackSubmitSigner,
    receipt: &GameParticipantReceiptV1,
    key: &str,
) {
    let WorldServicePayloadV1::GameParticipantRegister { owner, admission } =
        payload(root, identity, "account-a", "register-a", key)
    else {
        unreachable!()
    };
    let read = sign_read_request(
        VIEW_JSON_PATH,
        ReadWorldViewRequest {
            contract_version: 1,
            observation_nonce: Some("cd".repeat(32)),
            formal_anchor: None,
            world: identity.clone(),
            scope_id: format!("agent:{}", receipt.agent_id),
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        },
        key,
    )
    .unwrap();
    let request = ParticipantOwnerViewRequestV1 {
        owner,
        admission,
        read,
    };
    let response = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
        .post(format!("{endpoint}{PARTICIPANT_OWNER_VIEW_PATH}"))
        .json(&request)
        .send()
        .unwrap();
    let fresh = pin(root, identity);
    let projection_error = oasis7::world_service::projection::from_committed_world(
        &fresh.world,
        Some(&receipt.agent_id),
        &receipt.owner_public_key,
    )
    .err();
    assert_eq!(
        response.status().as_u16(),
        200,
        "a canonical participant owner read must not require provider provisioning: {projection_error:?}"
    );
    let signed: SignedServiceResponse<ReadWorldViewResponse<Value>> = response.json().unwrap();
    oasis7_client_api::world_service::verify_service_response(
        VIEW_JSON_PATH,
        &oasis7_client_api::world_service::request_digest(VIEW_JSON_PATH, &request.read).unwrap(),
        &signed,
        &signer.public_key_hex,
    )
    .unwrap();
    signed.payload.validate(&request.read.request).unwrap();
    assert_eq!(
        signed.payload.view["canonical_agent_owner"]["agent_id"],
        receipt.agent_id
    );
    assert!(signed.payload.view["agent_context"].is_null());
    assert!(signed.payload.view["game_task_control"].is_null());
    let pinned = pin(root, identity);
    assert!(pinned.world.capability_invocation_contexts().is_empty());
    assert!(
        pinned
            .world
            .canonical_agent_service_context(&receipt.agent_id)
            .is_err(),
        "provider execution remains strict"
    );
}
